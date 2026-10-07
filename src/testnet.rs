use clap::Args;

#[derive(Args)]
pub struct TestnetArgs {}

pub fn run_cli(_args: TestnetArgs) -> i32 {
    eprintln!("testnet mode is not implemented yet");
    4
}
