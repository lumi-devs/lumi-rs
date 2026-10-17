use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().collect();
    let Some(cmd) = args.get(1).map(String::as_str) else {
        eprintln!("Usage: lumi <shard|api|rest|all>");
        return ExitCode::FAILURE;
    };

    match cmd {
        "shard" | "api" | "rest" | "all" => {
            eprintln!("not implemented: {cmd} mode");
            ExitCode::from(74)
        }
        unexpected => {
            eprintln!("unknown command: {unexpected}");
            ExitCode::from(65)
        }
    }
}
