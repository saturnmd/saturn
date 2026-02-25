mod application;
mod message;
mod rich_text;

use crate::application::Strelka;

pub const APP_TITLE: &str = "Strelka";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

pub fn main() -> iced::Result {
    iced::daemon(Strelka::new, Strelka::update, Strelka::view)
        .title(Strelka::title)
        .subscription(Strelka::subscription)
        .run()
}
