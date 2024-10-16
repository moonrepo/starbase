use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct EntryProps<'a> {
    pub title: String,
    pub content: Option<AnyElement<'a>>,
    pub children: Vec<AnyElement<'a>>,
}

#[component]
pub fn Entry<'a>(props: &mut EntryProps<'a>) -> impl Into<AnyElement<'a>> {
    assert!(
        !(props.content.is_some() && !props.children.is_empty()),
        "Cannot use content and children props together"
    );

    let prefix = element! {
        Box(margin_right: 1) {
            Text(content: format!("{}:", props.title))
        }
    };

    // Stacked when children
    if !props.children.is_empty() {
        return element! {
            Box(flex_direction: FlexDirection::Column) {
                #(prefix)

                Box(padding_left: 2) {
                    #(&mut props.children)
                }
            }
        };
    }

    // Grouped when just the content
    element! {
        Box {
            #(prefix)
            Box {
                #(&mut props.content)
            }
        }
    }
}
