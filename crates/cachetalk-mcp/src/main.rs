use std::io::{self, BufReader};
use std::process::ExitCode;

use cachetalk_mcp::{McpServer, VERSION};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    if let Some(arg) = args.next() {
        match arg.as_str() {
            "-V" | "--version" => {
                println!("cachetalk-mcp {VERSION}");
                return ExitCode::SUCCESS;
            }
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("cachetalk-mcp: unknown argument: {other}");
                eprintln!("Try 'cachetalk-mcp --help' for more information.");
                return ExitCode::from(2);
            }
        }
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut server = McpServer::cache_occupancy();
    server.serve(BufReader::new(stdin.lock()), stdout.lock());
    ExitCode::SUCCESS
}

fn print_help() {
    println!("cachetalk-mcp {VERSION}");
    println!();
    println!("Usage: cachetalk-mcp [OPTIONS]");
    println!();
    println!("MCP server exposing cachetalk covert channel tools over stdio.");
    println!();
    println!("Options:");
    println!("  -h, --help       Print help");
    println!("  -V, --version    Print version");
}
