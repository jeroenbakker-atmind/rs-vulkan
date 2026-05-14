mod app;
mod texture;

use winit::event::WindowEvent;
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};

use app::App;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let config = match app::parse_args(&args) {
        Some(c) => c,
        None => return Ok(()),
    };

    let event_loop = EventLoop::new()?;

    let mut slide_app: Option<App> = None;

    #[allow(deprecated)]
    event_loop.run(move |event, elwt| {
        match event {
            winit::event::Event::Resumed => {
                if slide_app.is_none() {
                    match app::create_app(config.clone(), elwt) {
                        Ok(app) => {
                            slide_app = Some(app);
                        }
                        Err(e) => {
                            eprintln!("Failed to create app: {e}");
                            elwt.exit();
                        }
                    }
                }
            }
            winit::event::Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    elwt.exit();
                }
                WindowEvent::KeyboardInput {
                    event:
                        winit::event::KeyEvent {
                            physical_key: PhysicalKey::Code(keycode),
                            state: winit::event::ElementState::Pressed,
                            ..
                        },
                    ..
                } => {
                    if let Some(app) = &mut slide_app {
                        let changed = match keycode {
                            KeyCode::Escape => {
                                elwt.exit();
                                false
                            }
                            KeyCode::ArrowLeft => {
                                app.prev_chapter();
                                true
                            }
                            KeyCode::ArrowRight => {
                                app.next_chapter();
                                true
                            }
                            KeyCode::ArrowDown => {
                                app.next_slide();
                                true
                            }
                            KeyCode::ArrowUp => {
                                app.prev_slide();
                                true
                            }
                            KeyCode::Space => {
                                app.next_slide();
                                true
                            }
                            _ => false,
                        };
                        if changed {
                            app.request_redraw();
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let Some(app) = &mut slide_app {
                        if let Err(e) = app.render() {
                            eprintln!("Render error: {e}");
                        }
                    }
                }
                _ => {}
            },
            winit::event::Event::AboutToWait => {
                if let Some(app) = &mut slide_app {
                    let was_transitioning = app.is_transitioning;
                    app.update();
                    if was_transitioning || app.is_transitioning {
                        app.request_redraw();
                    }
                }
            }
            _ => {}
        }
    })?;

    Ok(())
}
