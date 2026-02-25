mod app;
mod game;
mod hyperbolic;
mod render;
mod sim;
mod ui;

use app::App;
use hyperbolic::poincare::TilingConfig;
use winit::event_loop::EventLoop;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let (p, q) = if args.len() >= 3 {
        (
            args[1].parse().expect("p must be a positive integer"),
            args[2].parse().expect("q must be a positive integer"),
        )
    } else {
        (4, 5)
    };

    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::new(TilingConfig::new(p, q));
    event_loop.run_app(&mut app).expect("event loop error");
}
