use crate::api::{BlockLike, BlockPersistence, BlockProvider};
use crate::monitor::ProgressMonitor;
use crate::settings::IndexerSettings;
use crate::syncer::ChainSyncer;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

pub struct Scheduler<FB: Send + Sync + 'static, TB: BlockLike + 'static> {
    pub syncer: ChainSyncer<FB, TB>,
}

impl<FB: Send + Sync + 'static, TB: BlockLike + 'static> Scheduler<FB, TB> {
    pub fn new(block_provider: Arc<dyn BlockProvider<FB, TB>>, block_persistence: Arc<dyn BlockPersistence<TB>>) -> Self {
        let syncer = ChainSyncer { block_provider, block_persistence, monitor: Arc::new(ProgressMonitor::new(1000)) };
        Scheduler { syncer }
    }

    pub async fn schedule(&self, indexer_conf: &IndexerSettings) {
        async {
            let mut interval = time::interval(Duration::from_secs(1));
            loop {
                self.syncer.sync(indexer_conf.min_batch_size, indexer_conf.processing_parallelism.clone().into()).await;
                interval.tick().await;
            }
        }
        .await;
    }
}
