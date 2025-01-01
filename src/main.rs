use std::sync::Arc;

use clap::Parser;

mod client;
mod server;
pub mod shared;

#[derive(Parser, Debug)]
#[command(version, long_about = None)]
struct Args {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug)]
enum SubCommand {
    #[command(about = "Start the server")]
    Server(server::Args),

    #[command(about = "Start the client")]
    Client(client::Args),
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    match args.subcmd {
        SubCommand::Server(args) => {
            let mut server = server::Server::new(args.address, args.port).await;
            server.run().await;
        }
        SubCommand::Client(args) => {
            let client = Arc::new(client::Client::new(args.address, args.port).await);
            let cloned_client = client.clone();
            tokio::spawn(async move {
                cloned_client.handle().await;
            });
            client.run_ui().await;
        }
    }
}
