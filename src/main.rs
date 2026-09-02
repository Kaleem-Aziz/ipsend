use std::io;
use std::env;

mod network;
mod packet;

fn main() -> io::Result<()> {

    let args: Vec<String> = env::args().collect();
    
    let query = &args[1];
    let ip_addr = &args[2];


    
    if query == "1" {
        let client = network::NetworkClient::setup(ip_addr)?;
        client.start_listening()?;
    } else {
        network::NetworkClient::send_message(ip_addr)?;
    }

    
    Ok(())
}
