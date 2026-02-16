use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

pub fn render_markdown_html(content: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(content, options).map(|event| match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: sanitize_link_destination(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: sanitize_image_source(dest_url),
            title,
            id,
        }),
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(CowStr::from(raw.into_string())),
        other => other,
    });
    let mut output = String::new();
    html::push_html(&mut output, parser);
    output
}

fn sanitize_link_destination(dest_url: CowStr<'_>) -> CowStr<'static> {
    let value = dest_url.into_string();
    if is_safe_destination(&value, true) {
        CowStr::from(value)
    } else {
        CowStr::from("#")
    }
}

fn sanitize_image_source(dest_url: CowStr<'_>) -> CowStr<'static> {
    let value = dest_url.into_string();
    if is_safe_destination(&value, false) {
        CowStr::from(value)
    } else {
        CowStr::from("")
    }
}

fn is_safe_destination(value: &str, allow_mailto: bool) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("//") {
        return false;
    }
    if lower.starts_with('#')
        || lower.starts_with('/')
        || lower.starts_with("./")
        || lower.starts_with("../")
    {
        return true;
    }
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("axiom://")
        || (allow_mailto && lower.starts_with("mailto:"))
    {
        return true;
    }

    !lower.contains(':')
}
