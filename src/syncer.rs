use crate::api::BlockHeaderLike;
use crate::api::{BlockLike, BlockPersistence};
use crate::api::{BlockProvider, ChainSyncError};
use crate::info;
use crate::monitor::ProgressMonitor;
use futures::StreamExt;
use min_batch::ext::MinBatchExt;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct ChainSyncer<FB: Send + Sync + 'static, TB: BlockLike + 'static> {
    pub block_provider: Arc<dyn BlockProvider<FB, TB>>,
    pub block_persistence: Arc<dyn BlockPersistence<TB>>,
    pub monitor: Arc<ProgressMonitor>,
}

impl<FB: Send + Sync + 'static, TB: BlockLike + 'static> ChainSyncer<FB, TB> {
    pub fn new(block_provider: Arc<dyn BlockProvider<FB, TB>>, block_persistence: Arc<dyn BlockPersistence<TB>>) -> Self {
        Self { block_provider, block_persistence, monitor: Arc::new(ProgressMonitor::new(1000)) }
    }

    pub async fn sync(&self, min_batch_size: usize, _processing_par: usize) {
        let block_provider = Arc::clone(&self.block_provider);
        let persistence = Arc::clone(&self.block_persistence);
        let monitor = Arc::clone(&self.monitor);

        let chain_tip_header = block_provider.get_chain_tip().await.expect("Failed to get chain tip header");
        let last_header = persistence.get_last_header().expect("Failed to get last header");
        let chain_tip_for_persist = chain_tip_header.clone();

        type Batch<T> = (Vec<T>, usize);
        let (fetch_tx, mut fetch_rx) = mpsc::channel::<FB>(8);
        let (proc_tx, mut proc_rx) = mpsc::channel::<Batch<TB>>(8);

        // Fetch stage
        let fetch_provider = Arc::clone(&self.block_provider);
        let fetch_handle = tokio::spawn(async move {
            fetch_provider
                .stream(chain_tip_header.clone(), last_header)
                .await
                .for_each(|block| {
                    let fetch_tx = fetch_tx.clone();
                    async move {
                        fetch_tx.send(block).await.expect("Processor dropped fetch channel");
                    }
                })
                .await;
        });

        // Process + batch stage
        let process_provider = Arc::clone(&self.block_provider);
        let process_handle = tokio::spawn(async move {
            let mut stream =
                futures::stream::poll_fn(|cx| fetch_rx.poll_recv(cx))
                    .map(|block| process_provider.process_block(&block).expect("Failed to process block"))
                    .min_batch_with_weight(min_batch_size, |block| block.weight() as usize);

            while let Some(batch) = stream.next().await {
                proc_tx.send(batch).await.expect("Persistence worker dropped processing channel");
            }
        });

        // Persist stage
        let persist_handle = tokio::spawn(async move {
            while let Some((block_batch, batch_weight)) = proc_rx.recv().await {
                let chain_link = block_batch.last().is_some_and(|curr_block| curr_block.header().height() + 100 > chain_tip_for_persist.height());

                let last_block = block_batch.last().expect("Block batch should not be empty");
                let height = last_block.header().height();
                let timestamp = last_block.header().timestamp();

                monitor.log(height, timestamp, block_batch.len(), &batch_weight);
                Self::persist_blocks(block_batch, chain_link, Arc::clone(&block_provider), Arc::clone(&persistence))
                    .unwrap_or_else(|e| panic!("Unable to persist blocks due to {}", e.error));
            }
        });

        fetch_handle.await.expect("Fetcher failed");
        process_handle.await.expect("Processor failed");
        persist_handle.await.expect("Persistence worker failed");
    }

    fn chain_link(block: TB, block_provider: Arc<dyn BlockProvider<FB, TB>>, block_persistence: Arc<dyn BlockPersistence<TB>>) -> Result<Vec<TB>, ChainSyncError> {
        let header = block.header();
        let prev_headers = block_persistence.get_header_by_hash(header.prev_hash())?;

        // Base case: genesis
        if header.height() == 1 {
            return Ok(vec![block]);
        }

        // If the DB already has the direct predecessor, we can stop here
        if prev_headers.first().map(|ph| ph.height() == header.height() - 1).unwrap_or(false) {
            return Ok(vec![block]);
        }

        // Otherwise we need to fetch the parent and prepend it
        if prev_headers.is_empty() {
            info!("Fork detected at {}@{}, downloading parent {}", header.height(), hex::encode(header.hash()), hex::encode(header.prev_hash()),);

            // fetch parent
            let parent_header = header.clone();
            let parent_block = block_provider.get_processed_block(parent_header)?;
            // recurse to build the earlier part of the chain
            let mut chain = Self::chain_link(parent_block, block_provider, block_persistence)?;
            // now append our current block at the end
            chain.push(block);
            return Ok(chain);
        }

        // If we got here, there were multiple candidates in DB → panic or handle specially
        panic!("Unexpected condition in chain_link: multiple parent candidates for {}@{}", header.height(), hex::encode(header.hash()));
    }

    pub fn persist_blocks(blocks: Vec<TB>, do_chain_link: bool, block_provider: Arc<dyn BlockProvider<FB, TB>>, block_persistence: Arc<dyn BlockPersistence<TB>>) -> Result<(), ChainSyncError> {
        for block in blocks {
            // consume each block by value, build its chain
            let chain = if do_chain_link { Self::chain_link(block, Arc::clone(&block_provider), Arc::clone(&block_persistence))? } else { vec![block] };

            match chain.len() {
                0 => unreachable!("chain_link never returns empty Vec"),
                1 => block_persistence.store_blocks(chain)?,
                _ => block_persistence.update_blocks(chain)?,
            }
        }
        Ok(())
    }
}
