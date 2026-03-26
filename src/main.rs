use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if let Some(help) = clyde::cli::help_from_args(args.iter().map(String::as_str)) {
        println!("{help}");
        return Ok(());
    }

    let cli = clyde::cli::Cli::parse();
    clyde::app::run(cli)
}
