mod generate_json_schema;

fn main() {
    tracing_subscriber::fmt().init();
    let check_flag = std::env::args().any(|arg| &arg == "--check");
    generate_json_schema::generate_json_schema(check_flag);
}
