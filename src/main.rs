mod adapters;
mod resume;
mod search;

fn main() {
    let sessions = adapters::load_all_sessions();
    println!("Found {} sessions", sessions.len());
}
