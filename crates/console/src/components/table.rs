use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

fn align_to_justify(align: TextAlign) -> JustifyContent {
    match align {
        TextAlign::Left => JustifyContent::Start,
        TextAlign::Right => JustifyContent::End,
        TextAlign::Center => JustifyContent::Center,
    }
}

struct TableContext {
    pub col_data: Vec<TableHeader>,
    pub term_width: u16,
}

#[derive(Clone, Default)]
pub struct TableHeader {
    pub align: TextAlign,
    pub label: String,
    pub width: Size,

    above_width: Option<u16>,
    below_width: Option<u16>,
}

impl TableHeader {
    pub fn new(label: &str, width: Size) -> Self {
        Self {
            label: label.to_owned(),
            width,
            ..Default::default()
        }
    }

    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    pub fn hide_above(mut self, width: u16) -> Self {
        self.above_width = Some(width);
        self
    }

    pub fn hide_below(mut self, width: u16) -> Self {
        self.below_width = Some(width);
        self
    }
}

impl From<&str> for TableHeader {
    fn from(value: &str) -> Self {
        Self {
            label: value.into(),
            ..Default::default()
        }
    }
}

#[derive(Default, Props)]
pub struct TableProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub headers: Vec<TableHeader>,
}

#[component]
pub fn Table<'a>(
    props: &mut TableProps<'a>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'a>> + use<'a> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let (term_width, _) = hooks.use_terminal_size();
    let context = TableContext {
        col_data: props.headers.clone(),
        term_width,
    };

    element! {
        ContextProvider(value: Context::owned(context)) {
            View(
                border_color: theme.border_color,
                border_style: BorderStyle::Round,
                flex_direction: FlexDirection::Column,
                width: Size::Auto,
            ) {
                View(
                    border_edges: Edges::Bottom,
                    border_color: theme.border_color,
                    border_style: BorderStyle::Round,
                    gap: 2
                ) {
                    #(props.headers.iter().enumerate().map(|(index, header)| {
                        element! {
                            TableCol(col: index as i32, key: header.label.clone()) {
                                Text(
                                    content: header.label.clone(),
                                    weight: Weight::Bold,
                                )
                            }
                        }
                    }))
                }

                #(&mut props.children)
            }
        }
    }
}

#[derive(Default, Props)]
pub struct TableRowProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub row: i32,
}

#[component]
pub fn TableRow<'a>(
    props: &mut TableRowProps<'a>,
    hooks: Hooks,
) -> impl Into<AnyElement<'a>> + use<'a> {
    let theme = hooks.use_context::<ConsoleTheme>();

    element! {
        View(
            background_color: if props.row % 2 == 0 {
                None
            } else {
                Some(theme.bg_alt_color)
            },
            gap: 2
        ) {
            #(&mut props.children)
        }
    }
}

#[derive(Default, Props)]
pub struct TableColProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub col: i32,
}

#[component]
pub fn TableCol<'a>(
    props: &mut TableColProps<'a>,
    hooks: Hooks,
) -> impl Into<AnyElement<'a>> + use<'a> {
    let context = hooks.use_context::<TableContext>();
    let attrs = context
        .col_data
        .get(props.col as usize)
        .unwrap_or_else(|| panic!("Unknown column index {}", props.col));

    if context.term_width > 0 {
        let hide = attrs
            .above_width
            .is_some_and(|above| context.term_width > above)
            || attrs
                .below_width
                .is_some_and(|below| context.term_width < below);

        if hide {
            return element!(View(display: Display::None));
        }
    }

    element! {
        View(
            flex_shrink: if attrs.width == Size::Auto {
                None
            } else {
                Some(0.0)
            },
            justify_content: align_to_justify(attrs.align),
            width: attrs.width,
        ) {
            #(&mut props.children)
        }
    }
}
