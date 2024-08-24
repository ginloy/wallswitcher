mod app;
mod cli;
mod render;

fn main() {
    env_logger::init();
    app::App::run().expect("Failed to run app");
}
