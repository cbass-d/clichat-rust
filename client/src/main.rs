#[allow(unused_imports, dead_code)]
use ratatui::{
    backend::{Backend, CrosstermBackend},
    crossterm::event::{self, KeyCode, KeyEventKind},
    layout::{Constraint, Direction, Layout, Rect},
    style::Color,
    DefaultTerminal, Terminal,
};
use std::{io::Write, net::TcpStream, time::Duration};
use tokio::{
    io::*,
    sync::broadcast::{Receiver, Sender},
};

use state_handler::{
    action::Action,
    state::{ConnectionStatus, State},
    StateHandler,
};
use tui::{
    app_router::AppRouter,
    components::component::{Component, ComponentRender},
    Event, Tui,
};

mod state_handler;
mod tui;

fn establish_connection(server: &str) -> Result<TcpStream> {
    let stream = TcpStream::connect(server)?;
    return Ok(stream);
}

async fn run(shutdown_tx: Sender<String>, shutdown_rx: &mut Receiver<String>) -> Result<()> {
    let (state_handler, mut state_rx) = StateHandler::new();
    let (tui, mut action_rx, mut tui_events) = Tui::new();
    let mut shutdown_rx_state = shutdown_rx.resubscribe();
    let mut shutdown_rx_tui = shutdown_rx.resubscribe();

    // State Handler
    let state_task = tokio::spawn(async move {
        // Create and send initial state to TUI
        let mut state = State::default();
        state_handler.state_tx.send(state.clone()).unwrap();
        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        let mut stream: Option<TcpStream> = None;

        loop {
            if state.exit {
                let _ = shutdown_tx.send(String::from("end"));
            }

            tokio::select! {
                _tick = ticker.tick() => {},
                action = action_rx.recv() => {
                    match action.unwrap() {
                        Action::Connect { addr } => {
                            match establish_connection(&addr) {
                                Ok(s) => {
                                    state.set_server(addr);
                                    stream = Some(s);
                                    state.set_connection_status(ConnectionStatus::Established);
                                    state.push_notification("[+] Successfully connected".to_string());
                                    state_handler.state_tx.send(state.clone()).unwrap();
                                },
                                Err(e) => {
                                    state.set_connection_status(ConnectionStatus::Bricked);
                                    let err = e.to_string();
                                    state.push_notification("[-] Failed to connect: ".to_string() + &err);
                                    state_handler.state_tx.send(state.clone()).unwrap();
                                },
                            }
                        },
                        Action::Disconnect => {},
                        Action::Send { data } => {
                            match stream {
                                Some(ref mut stream) => {
                                    stream.write_all(data.as_bytes());
                                },
                                None => {}
                            }
                            state_handler.state_tx.send(state.clone()).unwrap();
                        },
                        Action::Quit => {
                            state.exit();
                            state_handler.state_tx.send(state.clone()).unwrap();
                        }
                    }
                }
                _ = shutdown_rx_state.recv() => {
                    break;
                }
            }
        }
    });

    // TUI Handler
    let tui_task = tokio::spawn(async move {
        let mut terminal = Tui::setup_terminal();
        // Get initial State
        let state = state_rx.recv().await.unwrap();
        let mut app_router = AppRouter::new(&state, tui.action_tx);
        let _ = terminal.draw(|f| app_router.render(f, ()));

        loop {
            tokio::select! {
                event = tui_events.next() => {
                    match event.unwrap() {
                        Event::Key(key) => {
                            app_router.handle_key_event(key);
                        },
                        Event::Tick => {},
                        Event::Error => {},
                    }
                }
                state = state_rx.recv() => {
                    app_router = app_router.update(&state.unwrap());
                }
                _ = shutdown_rx_tui.recv() => {
                    break;
                }
            }

            let _ = terminal.draw(|f| app_router.render(f, ()));
        }
    });

    let (_, _) = tokio::join!(tui_task, state_task);

    Ok(())
}

fn shutdown() {
    println!("shutting down client");
}

#[tokio::main]
async fn main() -> Result<()> {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel(3);
    let mut shutdown_rx_main = shutdown_rx.resubscribe();
    tokio::spawn(async move {
        let _ = run(shutdown_tx, &mut shutdown_rx).await;
    })
    .await?;

    tokio::select! {
        _ = shutdown_rx_main.recv() => {
            ratatui::restore();
            shutdown();
        }
    }

    Ok(())
}
