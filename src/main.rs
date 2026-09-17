use std::io;
use clap::Parser;
use std::time::Duration;


mod args;
mod network;
mod packet;
mod stats;
mod network_packet;
mod output;


use crate::args::Cli; 
use crate::args::Mode; 

fn main() -> io::Result<()> {

    let cli = Cli::parse();

    match cli.command {
        Mode::Recv { addr, csv, verbose  } => {
            let client = network::NetworkClient::setup_server(&addr)?;
            client.start_listening(verbose, csv)?;

        }

        Mode::Send { addr, size, count, interval, pps, bps } => {

            if size > 65507 || size < 24 {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "Payload size must be between 24 & 65507 B"));
            }

            let iv = calculate_interval(&size, &pps, &interval, &bps)?;
            network::NetworkClient::send_message(&addr, &size, &count, &iv)?;
        }
    }
    
    Ok(())
}


fn calculate_interval(size: &usize, pps: &Option<u64>, interval: &Option<f64>, bps: &Option<u64>) 
                          -> io::Result<Duration> {
    
    const NS_PER_SEC: u64 = 1_000_000_000;            // unit conversion
    const DEFAULT_INTERVAL_NS: u64 = 1_000_000_000;   // 1 packet/sec

    let interval_ns: u64 = match (*pps, *interval, *bps) {
        (Some(p), _, _) if p > 0   => NS_PER_SEC / p,
        (_, Some(i), _) if i > 0.0 => (i * NS_PER_SEC as f64) as u64,
        (_, _, Some(b)) if b > 0   => *size as u64 * 8 * NS_PER_SEC / b,
        _                          => DEFAULT_INTERVAL_NS,
    };


    Ok(Duration::from_nanos(interval_ns))

}


