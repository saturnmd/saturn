use iced::advanced::text as core_text;
use iced::widget::canvas;
use iced::widget::canvas::{Canvas, Frame};
use iced::{Color, Length, Pixels, Point, Rectangle, Renderer, Size, Theme, mouse};

/// Layout model for the rich text widget.
///
/// This is intentionally mostly zero-copy: it borrows all string slices.
pub struct RichLayout<'a> {
    pub paragraphs: Vec<Paragraph<'a>>,
    /// Optional background for the whole widget.
    pub background: Option<Color>,
    /// Extra vertical space between paragraphs, in logical pixels.
    pub paragraph_spacing: f32,
}

impl<'a> RichLayout<'a> {
    pub fn new(paragraphs: Vec<Paragraph<'a>>) -> Self {
        Self {
            paragraphs,
            background: None,
            paragraph_spacing: 8.0,
        }
    }
}

/// A paragraph is a sequence of inline spans.
pub struct Paragraph<'a> {
    pub spans: Vec<InlineSpan<'a>>,
}

impl<'a> Paragraph<'a> {
    pub fn new(spans: Vec<InlineSpan<'a>>) -> Self {
        Self { spans }
    }
}

/// Either a text span or an inline image placeholder.
pub enum InlineSpan<'a> {
    Text(TextSpan<'a>),
    Image(ImageSpan),
}

/// Text span with its own styling.
pub struct TextSpan<'a> {
    pub text: &'a str,
    pub size: f32,
    pub color: Color,
}

/// Inline image placeholder; rendered as a rectangle.
///
/// `width` and `height` are in logical pixels. The placeholder is aligned
/// on the text baseline of its line.
pub struct ImageSpan {
    pub width: f32,
    pub height: f32,
    pub color: Color,
}

/// Public entry point: build a rich text canvas widget.
pub fn rich_text<'a, Message: 'a>(
    layout: RichLayout<'a>,
) -> Canvas<RichProgram<'a>, Message, Theme, Renderer> {
    Canvas::new(RichProgram { layout })
        .width(Length::Fill)
        .height(Length::Fill)
}

/// Canvas program that knows how to render a `RichLayout`.
pub struct RichProgram<'a> {
    layout: RichLayout<'a>,
}

#[derive(Default)]
pub struct State {
    cache: canvas::Cache,
}

impl<Message> canvas::Program<Message, Theme, Renderer> for RichProgram<'_> {
    type State = State;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry<Renderer>> {
        // We cache all geometry for the given bounds. As long as the size
        // of the widget does not change, this will not be recomputed.
        let geometry = state
            .cache
            .draw(renderer, bounds.size(), |frame: &mut Frame<Renderer>| {
                draw_layout(&self.layout, frame, bounds);
            });

        vec![geometry]
    }
}

/// Very simple line layout. We do not use any heavy shaping here on purpose
/// to stay zero-copy and avoid additional allocations. Iced will still use
/// its cosmic-text based renderer under the hood for `fill_text`.
fn draw_layout<'a>(layout: &RichLayout<'a>, frame: &mut Frame<Renderer>, bounds: Rectangle) {
    let mut cursor_y = 0.0_f32;

    // Fill background if requested.
    if let Some(bg) = layout.background {
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), bg);
    }

    for (pi, paragraph) in layout.paragraphs.iter().enumerate() {
        // Lay out one paragraph into lines.
        let lines = layout_paragraph(paragraph, bounds.width);

        for line in &lines {
            let line_height = line
                .items
                .iter()
                .map(|item| match item.kind {
                    LineItemKind::Text { size, .. } => size,
                    LineItemKind::Image { height, .. } => height,
                })
                .fold(0.0_f32, f32::max)
                .max(1.0);

            let baseline = cursor_y + line_height * 0.8;

            let mut cursor_x = 0.0_f32;
            for item in &line.items {
                match &item.kind {
                    LineItemKind::Text {
                        content,
                        size,
                        color,
                    } => {
                        if content.is_empty() {
                            continue;
                        }

                        let mut text = canvas::Text::from(*content);
                        text.position = Point::new(cursor_x, baseline - size);
                        text.color = *color;
                        text.size = Pixels(*size);
                        text.line_height = core_text::LineHeight::Absolute(Pixels(*size));
                        text.font = iced::Font::default();
                        text.align_x = core_text::Alignment::Left;
                        text.align_y = iced::alignment::Vertical::Top;
                        text.shaping = core_text::Shaping::Advanced;

                        frame.fill_text(text);
                        cursor_x += item.width;
                    }
                    LineItemKind::Image {
                        width,
                        height,
                        color,
                    } => {
                        // Align bottom of image rectangle to text baseline.
                        let top = baseline - height;
                        frame.fill_rectangle(
                            Point::new(cursor_x, top),
                            Size::new(*width, *height),
                            *color,
                        );
                        cursor_x += item.width;
                    }
                }
            }

            cursor_y += line_height;
        }

        if pi + 1 != layout.paragraphs.len() {
            cursor_y += layout.paragraph_spacing;
        }
    }
}

/// A single laid out paragraph line.
struct Line<'a> {
    items: Vec<LineItem<'a>>,
}

struct LineItem<'a> {
    kind: LineItemKind<'a>,
    width: f32,
}

enum LineItemKind<'a> {
    Text {
        content: &'a str,
        size: f32,
        color: Color,
    },
    Image {
        width: f32,
        height: f32,
        color: Color,
    },
}

/// Very lightweight line-breaking: we assume a fixed per-character advance
/// proportional to the font size. This is good enough for display-only
/// usage and keeps the implementation cheap and zero-copy.
fn layout_paragraph<'a>(paragraph: &Paragraph<'a>, max_width: f32) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let mut current = Line { items: Vec::new() };
    let mut used = 0.0_f32;

    for span in &paragraph.spans {
        match span {
            InlineSpan::Text(TextSpan { text, size, color }) => {
                let char_width = size * 0.6_f32;

                // Split span into runs of "word" and "whitespace" to wrap at word
                // boundaries while keeping kerning within each run intact.
                let mut start_idx = 0;
                let mut current_is_space: Option<bool> = None;

                for (i, ch) in text
                    .char_indices()
                    .chain(std::iter::once((text.len(), '\0')))
                {
                    if i == start_idx {
                        current_is_space = Some(ch.is_whitespace());
                        continue;
                    }

                    let is_space = ch.is_whitespace();
                    let boundary =
                        i == text.len() || current_is_space.map(|s| s != is_space).unwrap_or(false);

                    if boundary {
                        let slice = &text[start_idx..i];
                        if !slice.is_empty() {
                            let slice_width = slice.chars().count() as f32 * char_width;

                            if used + slice_width > max_width && !current.items.is_empty() {
                                // Flush current line.
                                lines.push(current);
                                current = Line { items: Vec::new() };
                                used = 0.0;
                            }

                            current.items.push(LineItem {
                                kind: LineItemKind::Text {
                                    content: slice,
                                    size: *size,
                                    color: *color,
                                },
                                width: slice_width,
                            });
                            used += slice_width;
                        }

                        start_idx = i;
                        current_is_space = Some(is_space);
                    }
                }
            }
            InlineSpan::Image(ImageSpan {
                width,
                height,
                color,
            }) => {
                if used + *width > max_width && !current.items.is_empty() {
                    lines.push(current);
                    current = Line { items: Vec::new() };
                    used = 0.0;
                }

                current.items.push(LineItem {
                    kind: LineItemKind::Image {
                        width: *width,
                        height: *height,
                        color: *color,
                    },
                    width: *width,
                });
                used += *width;
            }
        }
    }

    if !current.items.is_empty() {
        lines.push(current);
    }

    lines
}
