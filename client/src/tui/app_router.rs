use super::components::component::{Component, ComponentRender};
use super::components::main_page::MainPage;
use crate::state_handler::{action::Action, state::State};

use crossterm::event::KeyEvent;
use ratatui::{prelude::*, Frame};
use tokio::sync::mpsc::UnboundedSender;

pub struct AppRouter {
    main_page: MainPage,
}

impl Component for AppRouter {
    fn new(state: &State, action_tx: UnboundedSender<Action>) -> Self
    where
        Self: Sized,
    {
        AppRouter {
            main_page: MainPage::new(state, action_tx),
        }
    }

    fn update(self, state: &State) -> Self
    where
        Self: Sized,
    {
        Self {
            main_page: self.main_page.update(state),
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        self.main_page.handle_key_event(key);
    }
}

impl ComponentRender<()> for AppRouter {
    fn render(&self, frame: &mut Frame, props: ()) {
        self.main_page.render(frame, props);
    }
}
