use clap::Parser;

#[derive(Parser, Debug)]
#[clap(version = "0.1.0")]
#[clap(name = "TCPServer")]
#[clap(about = "A simple TCP server", long_about = None)]
pub struct CliArgs {
    #[clap(short, long, default_value = "config.toml")]
    pub config: String,

    #[clap(short, long)]
    pub daemon: bool,

    #[clap(short, long, default_value_t = 1)]
    pub verbose: u8,
}

impl CliArgs {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
}
