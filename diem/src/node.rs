use crate::committee::Config as _;
use crate::committee::{Committee, Secret};
use crate::core::Core;
use crate::crypto::SignatureService;
use crate::error::{DiemError, DiemResult};
use crate::leader::LeaderElector;
use crate::mempool::Mempool;
use crate::network::{NetReceiver, NetSender};
use crate::store::Store;
use tokio::sync::mpsc::channel;

pub struct Parameters {
    pub timeout_delay: u64,
    pub sync_retry_delay: u64,
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            timeout_delay: 1_000,
            sync_retry_delay: 10_000,
        }
    }
}

pub struct Node;

impl Node {
    pub async fn make(
        committee_file: &str,
        key_file: &str,
        store_path: &str,
        parameters: Option<Parameters>,
    ) -> DiemResult<()> {
        // Read the committee and secret key from file.
        let committee = Committee::read(committee_file)?;
        let secret = Secret::read(key_file)?;

        // Retrieve node's information.
        let name = secret.name;
        let secret_key = secret.secret;
        let address = match committee.address(&name) {
            Some(address) => address,
            None => bail!(DiemError::ConfigError(
                committee_file.to_string(),
                "Node name in not in the committee".to_string()
            )),
        };

        // Load default parameters if none are specified.
        let parameters = match parameters {
            Some(parameters) => parameters,
            None => Parameters::default(),
        };

        // Make the data store.
        let store = Store::new(store_path).await?;

        // Run the signature service.
        let signature_service = SignatureService::new(secret_key).await;

        // Choose the mempool and leader election algorithm.
        let mempool = Mempool::new().await;
        let leader_elector = LeaderElector::new(committee.clone());

        // Create the commit channel from which we can read the sequence of
        // committed blocks.
        let (tx_commit, mut rx_commit) = channel(1000);

        // Now wire together the network sender, core, and network receiver.
        let network_channel = NetSender::make(name, committee.clone()).await;
        let core_channel = Core::make(
            name,
            committee,
            parameters,
            store,
            signature_service,
            leader_elector,
            mempool,
            network_channel,
            tx_commit,
        )
        .await;
        let () = NetReceiver::make(address, core_channel).await;

        // Sink the commits (we don't execute transactions).
        while rx_commit.recv().await.is_some() {}
        Ok(())
    }

    pub fn print_key_file(filename: &str) -> DiemResult<()> {
        Secret::new().write(filename)
    }
}
