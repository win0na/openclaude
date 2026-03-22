use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = openclaude::cli::Cli::parse();
    openclaude::app::run(cli)
}
