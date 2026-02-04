use clap::Parser;
use catacomb_ipc::{IpcMessage, send_message, CliToggle};

#[derive(Parser)]
#[command(name = "catacomb-debug")]
#[command(about = "Standalone debug tool for Catacomb")]
struct Cli {
    #[command(subcommand)]
    command: IpcMessage,
}

fn main() {
    let cli = Cli::parse();

    match send_message(&cli.command) {
        Ok(Some(reply)) => {
             match reply {
                IpcMessage::DebugTreeReply { tree } => println!("{}", tree),
                IpcMessage::DpmsReply { state: CliToggle::On } => println!("on"),
                IpcMessage::DpmsReply { state: CliToggle::Off } => println!("off"),
                IpcMessage::ActiveWindow { title, app_id } => println!("Title: {}\nApp ID: {}", title, app_id),
                IpcMessage::OutputInfo { width, height, refresh, scale, orientation: _ } => {
                    println!("{}x{}@{}mHz (scale: {})", width, height, refresh, scale);
                },
                IpcMessage::OutputModes { modes } => {
                    for mode in modes {
                        println!("{}x{}@{}mHz", mode.width, mode.height, mode.refresh);
                    }
                },
                _ => println!("Success: {:?}", reply),
            }
        }
        Ok(None) => {
             // For commands that don't return a value
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
