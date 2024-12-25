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
}

#[derive(Clone, Default)]
pub struct TableHeader {
    pub align: TextAlign,
    pub label: String,
    pub width: Size,
}

impl TableHeader {
    pub fn new(label: &str, width: Size) -> Self {
        Self {
            label: label.to_owned(),
            width,
            ..Default::default()
        }
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
pub fn Table<'a>(props: &mut TableProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let context = TableContext {
        col_data: props.headers.clone(),
    };

    element! {
        ContextProvider(value: Context::owned(context)) {
            Box(
                border_color: theme.border_color,
                border_style: BorderStyle::Round,
                flex_direction: FlexDirection::Column,
                width: Size::Auto,
            ) {
                Box(
                    border_edges: Edges::Bottom,
                    border_color: theme.border_color,
                    border_style: BorderStyle::Round,
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
pub fn TableRow<'a>(props: &mut TableRowProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();

    element! {
        Box(
            background_color: if props.row % 2 == 0 {
                None
            } else {
                Some(theme.bg_alt_color)
            }
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
pub fn TableCol<'a>(props: &mut TableColProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let context = hooks.use_context::<TableContext>();
    let attrs = context
        .col_data
        .get(props.col as usize)
        .unwrap_or_else(|| panic!("Unknown column index {}", props.col));

    element! {
        Box(
            justify_content: align_to_justify(attrs.align),
            padding_left: 1,
            // padding_right: 1,
            width: attrs.width,
        ) {
            #(&mut props.children)
        }
    }
}
