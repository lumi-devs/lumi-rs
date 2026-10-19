// Sample Rhai addon script
fn on_load() {
    log_info("ping_addon successfully loaded inside Rhai sandboxed runtime!");
}

fn handle_hello(author_name) {
    log_info("handle_hello invoked for user: " + author_name);
    return "👋 Hello, " + author_name + "! Greetings from your dynamic Rhai addon running in Rust!";
}
