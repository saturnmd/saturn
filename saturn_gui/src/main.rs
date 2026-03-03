#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod application;
mod message;
mod rich_text;
mod widget;

use crate::application::Application;

pub const APP_TITLE: &str = "Saturn";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

pub fn main() -> iced::Result {
    iced::daemon(Application::new, Application::update, Application::view)
        .title(Application::title)
        .subscription(Application::subscription)
        .run()
}
