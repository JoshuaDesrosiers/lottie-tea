//! `an1mbyt3-server [port]` — hosts a multiplayer game without the editor:
//! the same relay `ctx.net.host()` starts inside the editor. Players join
//! with `ctx.net.join("<this machine's address>:<port>")`.

fn main() {
    let port = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(an1mbyt3_net::DEFAULT_PORT);
    let server = match an1mbyt3_net::RelayServer::start(port) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("an1mbyt3-server: can't listen on port {port}: {e}");
            std::process::exit(1);
        }
    };
    println!("an1mbyt3-server listening on port {}", server.port());
    for address in an1mbyt3_net::lan_addresses() {
        println!("  players join with  {address}:{}", server.port());
    }
    // Serve until killed (Ctrl+C).
    let mut last = usize::MAX;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let count = server.player_count();
        if count != last {
            println!("{count} player(s) connected");
            last = count;
        }
    }
}
