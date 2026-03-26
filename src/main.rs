use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if let Some(help) = openclaude::cli::help_from_args(args.iter().map(String::as_str)) {
        println!("{help}");
        return Ok(());
    }

    let cli = openclaude::cli::Cli::parse();
    openclaude::app::run(cli)
}
