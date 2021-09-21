use super::CounterApp;
use orga::prelude::*;

pub async fn run_client() -> Result<()> {
    let client: TendermintClient<CounterApp> =
        TendermintClient::new("http://localhost:26657").unwrap();

    let mut client = CounterApp::create_client(client);

    client.increment().await
}
