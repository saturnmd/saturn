use iced::widget::{center, container};
use iced::{Color, Element, Font, Length, Subscription, Task};

use crate::message::{self, Message};
use crate::widget::rich_text::{self, ImageSpan, InlineSpan, Paragraph, RichLayout, TextSpan};
use tracing::{debug, info};

pub struct Application {
    window_id: iced::window::Id,
    layout: RichLayout<'static>,
}

impl Application {
    pub fn new() -> (Self, Task<Message>) {
        let (main_window_id, open_main_window) = iced::window::open(iced::window::Settings {
            exit_on_close_request: false,
            ..iced::window::Settings::default()
        });

        let tasks = vec![
            open_main_window
                .map(|_| Message::Window(message::WindowMessage::InitializedMainWindow)),
        ];

        // Static text used in the demo layout.
        let lorem_1: &'static str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
        Curabitur blandit tempus porttitor. Integer posuere erat a ante venenatis dapibus.";

        let lorem_2: &'static str = "Aenean eu leo quam. Pellentesque ornare sem lacinia quam venenatis vestibulum. \
        Cras mattis consectetur purus sit amet fermentum.";

        let default_font = Font::default();

        let mut layout: RichLayout<'static> = RichLayout::new(vec![
            Paragraph::new(vec![InlineSpan::Text(TextSpan {
                text: "Meeting notes — Project Saturn",
                size: 22.0,
                color: Color::from_rgb(0.1, 0.1, 0.1),
                font: default_font,
                bold: false,
                italic: false,
            })]),
            Paragraph::new(vec![
                InlineSpan::Text(TextSpan {
                    text: "Tag: ",
                    size: 14.0,
                    color: Color::from_rgb(0.3, 0.3, 0.3),
                    font: default_font,
                    bold: false,
                    italic: false,
                }),
                InlineSpan::Image(ImageSpan {
                    width: 36.0,
                    height: 16.0,
                    color: Color::from_rgb(0.85, 0.9, 1.0),
                }),
                InlineSpan::Text(TextSpan {
                    text: "  design, planning",
                    size: 14.0,
                    color: Color::from_rgb(0.4, 0.4, 0.4),
                    font: default_font,
                    bold: false,
                    italic: false,
                }),
            ]),
            Paragraph::new(vec![InlineSpan::Text(TextSpan {
                text: lorem_1,
                size: 16.0,
                color: Color::from_rgb(0.15, 0.15, 0.15),
                font: default_font,
                bold: false,
                italic: false,
            })]),
            Paragraph::new(vec![
                InlineSpan::Text(TextSpan {
                    text: "Important: ",
                    size: 16.0,
                    color: Color::from_rgb(0.8, 0.3, 0.0),
                    font: default_font,
                    bold: false,
                    italic: true,
                }),
                InlineSpan::Text(TextSpan {
                    text: lorem_2,
                    size: 16.0,
                    color: Color::from_rgb(0.15, 0.15, 0.15),
                    font: default_font,
                    bold: true,
                    italic: true,
                }),
            ]),
        ]);

        layout.paragraph_spacing = 10.0;

        (
            Self {
                window_id: main_window_id,
                layout,
            },
            Task::batch(tasks),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Window(msg) => match msg {
                message::WindowMessage::InitializedMainWindow => {
                    debug!("Main window initialized")
                }
                message::WindowMessage::Close(id) => {
                    let mut close_task = iced::window::close(id);
                    // Close an entire application if we trying to close main window
                    if id == self.window_id {
                        close_task = close_task.chain(self.exit());
                    }
                    return close_task;
                }
            },
            Message::None => {}
        }

        Task::none()
    }

    pub fn view(&self, _window_id: iced::window::Id) -> Element<'_, Message> {
        center(
            container(rich_text::editor_renderer::<Message, _>(
                self.layout.clone(),
            ))
            .width(Length::Fixed(600.0))
            .padding(16.0),
        )
        .into()
    }

    pub fn title(&self, _window_id: iced::window::Id) -> String {
        String::from("Saturn")
    }

    fn exit(&mut self) -> Task<Message> {
        info!("Closing application gracefully");

        iced::exit()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let tasks = vec![
            iced::window::close_requests()
                .map(|id| Message::Window(message::WindowMessage::Close(id))),
        ];
        Subscription::batch(tasks)
    }
}
