//! Editor renderer widget.
//!
//! A zero-copy widget that renders content from rich text or from an editor
//! (e.g. one using cosmic-text with ligatures and multistep parsing). Uses
//! the Widget trait for proper integration.

use std::marker::PhantomData;
use std::sync::{Mutex, OnceLock};

use cosmic_text::{
    Attrs, Buffer, Family as CtFamily, FontSystem, Metrics, Shaping, Style as CtStyle,
    Weight as CtWeight,
};

use iced::advanced::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::text as core_text;
use iced::advanced::text::Renderer as _;
use iced::advanced::widget::{self, Widget};
use iced::alignment;
use iced::font::{Family as IcedFamily, Style as FontStyle, Weight as FontWeight};
use iced::mouse;
use iced::{Color, Element, Font, Length, Pixels, Point, Rectangle, Renderer, Size, Theme};

/// Layout model for rich text. Zero-copy: it borrows all string slices.
#[derive(Clone)]
pub struct RichLayout<'a> {
    pub paragraphs: Vec<Paragraph<'a>>,
    pub background: Option<Color>,
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
#[derive(Clone)]
pub struct Paragraph<'a> {
    pub spans: Vec<InlineSpan<'a>>,
}

impl<'a> Paragraph<'a> {
    pub fn new(spans: Vec<InlineSpan<'a>>) -> Self {
        Self { spans }
    }
}

/// Either a text span or an inline image placeholder.
#[derive(Clone)]
pub enum InlineSpan<'a> {
    Text(TextSpan<'a>),
    Image(ImageSpan),
}

/// Text span with its own styling.
#[derive(Clone)]
pub struct TextSpan<'a> {
    pub text: &'a str,
    pub size: f32,
    pub color: Color,
    /// Font used for this span.
    pub font: Font,
    /// Whether to render this span in bold.
    pub bold: bool,
    /// Whether to render this span in italic.
    pub italic: bool,
}

/// Inline image placeholder.
#[derive(Clone, Copy)]
pub struct ImageSpan {
    pub width: f32,
    pub height: f32,
    pub color: Color,
}

// -----------------------------------------------------------------------------
// Render content (zero-copy, editor-friendly)
// -----------------------------------------------------------------------------

/// Zero-copy render content. Can be produced by `RichLayout` or by an editor
/// using cosmic-text (ligatures, proper shaping).
pub struct RenderContent<'a> {
    pub lines: Vec<LineContent<'a>>,
    pub width: f32,
    pub height: f32,
    pub background: Option<Color>,
}

/// A single line of positioned runs.
pub struct LineContent<'a> {
    pub runs: Vec<RunContent<'a>>,
    pub y: f32,
    pub height: f32,
}

/// A positioned run (text or image).
pub enum RunContent<'a> {
    Text {
        text: &'a str,
        x: f32,
        font_size: f32,
        color: Color,
        font: Font,
        bold: bool,
        italic: bool,
    },
    Image {
        x: f32,
        width: f32,
        height: f32,
        color: Color,
    },
}

/// Source of render content. Implemented by `RichLayout` (simple display) and
/// by editors using cosmic-text (ligatures, multistep parsing).
pub trait RenderSource<'a> {
    fn layout(&self, max_width: f32) -> RenderContent<'a>;
}

impl<'a> RenderSource<'a> for RichLayout<'a> {
    fn layout(&self, max_width: f32) -> RenderContent<'a> {
        let mut lines_out = Vec::new();
        let mut cursor_y = 0.0_f32;
        let mut max_width_seen = 0.0_f32;

        for (pi, paragraph) in self.paragraphs.iter().enumerate() {
            let computed = layout_paragraph(paragraph, max_width);

            for line in computed {
                let max_text_size = line
                    .items
                    .iter()
                    .filter_map(|i| match &i.kind {
                        LineItemKind::Text { size, .. } => Some(*size),
                        _ => None,
                    })
                    .fold(0.0_f32, f32::max)
                    .max(1.0);
                let max_image_height = line
                    .items
                    .iter()
                    .filter_map(|i| match &i.kind {
                        LineItemKind::Image { height, .. } => Some(*height),
                        _ => None,
                    })
                    .fold(0.0_f32, f32::max);
                let descender_depth = max_text_size * 0.25;
                let line_height = (max_text_size * 1.25)
                    .max(max_image_height + descender_depth)
                    .max(1.0);
                let line_bottom = cursor_y + line_height;
                let glyph_bottom = line_bottom - descender_depth;

                let mut runs = Vec::new();
                let mut cursor_x = 0.0_f32;
                for item in &line.items {
                    match &item.kind {
                        LineItemKind::Text {
                            content,
                            size,
                            color,
                            font,
                            bold,
                            italic,
                        } => {
                            if !content.is_empty() {
                                runs.push(RunContent::Text {
                                    text: content,
                                    x: cursor_x,
                                    font_size: *size,
                                    color: *color,
                                    font: *font,
                                    bold: *bold,
                                    italic: *italic,
                                });
                            }
                            cursor_x += item.width;
                        }
                        LineItemKind::Image {
                            width,
                            height,
                            color,
                        } => {
                            runs.push(RunContent::Image {
                                x: cursor_x,
                                width: *width,
                                height: *height,
                                color: *color,
                            });
                            cursor_x += item.width;
                        }
                    }
                }
                max_width_seen = max_width_seen.max(cursor_x);

                lines_out.push(LineContent {
                    runs,
                    y: glyph_bottom,
                    height: line_height,
                });
                cursor_y += line_height;
            }

            if pi + 1 != self.paragraphs.len() {
                cursor_y += self.paragraph_spacing;
            }
        }

        RenderContent {
            lines: lines_out,
            width: max_width_seen.max(1.0),
            height: cursor_y.max(1.0),
            background: self.background,
        }
    }
}

// -----------------------------------------------------------------------------
// Internal layout (simple line-breaking, zero-copy)
// -----------------------------------------------------------------------------

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
        font: Font,
        bold: bool,
        italic: bool,
    },
    Image {
        width: f32,
        height: f32,
        color: Color,
    },
}

fn attrs_from_font(font: Font, bold: bool, italic: bool) -> Attrs<'static> {
    let mut attrs = Attrs::new();

    // Family
    let family = match font.family {
        IcedFamily::Name(name) => CtFamily::Name(name),
        IcedFamily::Serif => CtFamily::Serif,
        IcedFamily::SansSerif => CtFamily::SansSerif,
        IcedFamily::Cursive => CtFamily::Cursive,
        IcedFamily::Fantasy => CtFamily::Fantasy,
        IcedFamily::Monospace => CtFamily::Monospace,
    };
    attrs = attrs.family(family);

    // Weight
    // Map to a coarse weight; cosmic-text does not expose the same enum
    // variants as iced, so we approximate: bold vs normal.
    let is_bold = matches!(
        font.weight,
        FontWeight::Bold | FontWeight::Semibold | FontWeight::ExtraBold | FontWeight::Black
    ) || bold;
    let weight = if is_bold {
        CtWeight::BOLD
    } else {
        CtWeight::NORMAL
    };
    attrs = attrs.weight(weight);

    // Style
    let base_style = match font.style {
        FontStyle::Normal => CtStyle::Normal,
        FontStyle::Italic => CtStyle::Italic,
        FontStyle::Oblique => CtStyle::Oblique,
    };
    let style = if italic { CtStyle::Italic } else { base_style };
    attrs.style(style)
}

/// Measure text width using cosmic-text for the given logical font.
fn measure_run(text: &str, font_size: f32, font: Font, bold: bool, italic: bool) -> f32 {
    static FONT_SYSTEM: OnceLock<Mutex<FontSystem>> = OnceLock::new();
    let font_system = FONT_SYSTEM.get_or_init(|| Mutex::new(FontSystem::new()));
    let mut font_system = font_system.lock().expect("font system lock");

    let metrics = Metrics::new(font_size, font_size * 1.25);
    let mut buffer = Buffer::new(&mut *font_system, metrics);
    buffer.set_size(&mut *font_system, Some(f32::MAX), None);
    let attrs = attrs_from_font(font, bold, italic);
    buffer.set_text(&mut *font_system, text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut *font_system, true);

    let mut width = 0.0_f32;
    for run in buffer.layout_runs() {
        if let (Some(first), Some(last)) = (run.glyphs.first(), run.glyphs.last()) {
            width += last.x + last.w - first.x;
        }
    }
    width
}

fn layout_paragraph<'a>(paragraph: &Paragraph<'a>, max_width: f32) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let mut current = Line { items: Vec::new() };
    let mut used = 0.0_f32;

    for span in &paragraph.spans {
        match span {
            InlineSpan::Text(TextSpan {
                text,
                size,
                color,
                font,
                bold,
                italic,
            }) => {
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
                            let slice_width = measure_run(slice, *size, *font, *bold, *italic);

                            if used + slice_width > max_width && !current.items.is_empty() {
                                lines.push(current);
                                current = Line { items: Vec::new() };
                                used = 0.0;
                            }

                            current.items.push(LineItem {
                                kind: LineItemKind::Text {
                                    content: slice,
                                    size: *size,
                                    color: *color,
                                    font: *font,
                                    bold: *bold,
                                    italic: *italic,
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

// -----------------------------------------------------------------------------
// Editor renderer widget
// -----------------------------------------------------------------------------

fn resolve_font(base: Font, bold: bool, italic: bool) -> Font {
    let mut font = base;
    if bold {
        font.weight = FontWeight::Bold;
    }
    if italic {
        font.style = FontStyle::Italic;
    }
    font
}

/// Build an editor renderer widget from a render source.
pub fn editor_renderer<'a, Message, S>(source: S) -> EditorRenderer<'a, Message, S>
where
    S: RenderSource<'a>,
{
    EditorRenderer {
        source,
        _phantom: PhantomData,
    }
}

/// Widget that renders content from a `RenderSource` (e.g. `RichLayout` or
/// an editor using cosmic-text). Zero-copy: borrows all text.
pub struct EditorRenderer<'a, Message, S>
where
    S: RenderSource<'a>,
{
    source: S,
    _phantom: PhantomData<(&'a (), fn() -> Message)>,
}

impl<'a, Message, S> Widget<Message, Theme, Renderer> for EditorRenderer<'a, Message, S>
where
    S: RenderSource<'a> + 'a,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let max_width = limits.max().width;
        let content = self.source.layout(max_width);
        let size = Size::new(content.width, content.height);
        layout::Node::new(size)
    }

    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let content = self.source.layout(bounds.width);
        let clip_bounds = bounds.intersection(viewport).unwrap_or(bounds);

        // Background
        if let Some(bg) = content.background {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    ..renderer::Quad::default()
                },
                iced::Background::Color(bg),
            );
        }

        // Content
        for line in &content.lines {
            for run in &line.runs {
                match run {
                    RunContent::Text {
                        text,
                        x,
                        font_size,
                        color,
                        font,
                        bold,
                        italic,
                    } => {
                        if text.is_empty() {
                            continue;
                        }

                        let prim = core_text::Text {
                            content: text.to_string(),
                            bounds: Size::new(f32::MAX, *font_size),
                            size: Pixels(*font_size),
                            line_height: core_text::LineHeight::Absolute(Pixels(*font_size)),
                            font: resolve_font(*font, *bold, *italic),
                            align_x: core_text::Alignment::Left,
                            align_y: alignment::Vertical::Top,
                            shaping: core_text::Shaping::Advanced,
                            wrapping: core_text::Wrapping::None,
                            hint_factor: None,
                        };

                        let position = Point::new(bounds.x + x, bounds.y + line.y - font_size);
                        renderer.fill_text(prim, position, *color, clip_bounds);
                    }
                    RunContent::Image {
                        x,
                        width,
                        height,
                        color,
                    } => {
                        let quad_bounds = Rectangle {
                            x: bounds.x + x,
                            y: bounds.y + line.y - height,
                            width: *width,
                            height: *height,
                        };
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: quad_bounds,
                                ..renderer::Quad::default()
                            },
                            iced::Background::Color(*color),
                        );
                    }
                }
            }
        }
    }
}

impl<'a, Message: 'a, S> From<EditorRenderer<'a, Message, S>>
    for Element<'a, Message, Theme, Renderer>
where
    S: RenderSource<'a> + 'a,
{
    fn from(widget: EditorRenderer<'a, Message, S>) -> Self {
        Element::new(widget)
    }
}

// No legacy `rich_text` helper is kept on purpose to avoid an unused API
// surface. Use `editor_renderer` or `widget::rich_text` instead.
