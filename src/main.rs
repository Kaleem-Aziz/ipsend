use std::io;
use std::env;
use clap::Parser;


mod args;
mod network;
mod packet;

use crate::args::Cli; 
use crate::args::Mode; 

fn main() -> io::Result<()> {

    let cli = Cli::parse();

    match cli.command {
        Mode::Recv { addr, capacity, csv, verbose  } => {
            let client = network::NetworkClient::setup(&addr)?;
            client.start_listening()?;
        
        }

        Mode::Send { addr, size, count, pps } => {
            network::NetworkClient::send_message(&addr, &size, &count, &pps)?;
        }
    }


    // if query == "1" {
    //     let client = network::NetworkClient::setup(ip_addr)?;
    //     client.start_listening()?;
    // } else {
    //     let bytes: usize = args[3]
    //         .parse::<usize>()
    //         .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("Could not parse argument: {}", e)))?;

    //     network::NetworkClient::send_message(ip_addr, &bytes)?;
    // }

    
    Ok(())
}


