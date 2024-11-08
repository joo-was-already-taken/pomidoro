use std::path::PathBuf;


#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(long = "config")]
    pub config_path: Option<PathBuf>,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    Start {
        #[arg(long = "id", default_value_t = 0)]
        server_id: u32,
    },
    Send {
        #[arg(long = "id", default_value_t = 0)]
        server_id: u32,

        #[command(subcommand)]
        request: Request,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum Request {
    Fetch {
        #[arg(value_parser = mustache::compile_str)]
        template: mustache::Template,
    },
    Toggle,
    Skip,
    Reset,
    Stop,
}
