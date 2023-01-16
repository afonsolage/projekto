use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct ApplyThemeStyle(pub Entity, pub ThemeStyle);

#[derive(Default, Debug, Clone)]
pub struct ThemeStyle(pub Vec<ThemeStyleProperty>);

#[derive(Debug, Clone)]
pub enum ThemeStyleProperty {
    Display(Display),
    PositionType(PositionType),
    Direction(Direction),
    FlexDirection(FlexDirection),
    FlexWrap(FlexWrap),
    AlignItems(AlignItems),
    AlignSelf(AlignSelf),
    AlignContent(AlignContent),
    JustifyContent(JustifyContent),
    Position(UiRect),
    Margin(UiRect),
    Padding(UiRect),
    Border(UiRect),
    FlexGrow(f32),
    FlexShrink(f32),
    FlexBasis(Val),
    Size(Size),
    MinSize(Size),
    MaxSize(Size),
    AspectRatio(Option<f32>),
    Overflow(Overflow),
}

pub(super) fn apply_theme_style(
    mut reader: EventReader<ApplyThemeStyle>,
    mut q: Query<&mut Style>,
) {
    for ApplyThemeStyle(entity, theme) in reader.iter() {
        if let Ok(mut style) = q.get_mut(*entity) {
            *style = ThemeStyler::from(style.clone()).apply(theme.clone());
        }
    }
}

struct ThemeStyler(Style);

impl ThemeStyler {
    pub fn from(style: Style) -> ThemeStyler {
        Self(style)
    }

    pub fn apply(self, theme: ThemeStyle) -> Style {
        let mut theme_style = self;
        for property in theme.0 {
            theme_style = theme_style.apply_property(property);
        }
        theme_style.0
    }

    fn apply_property(self, property: ThemeStyleProperty) -> Self {
        match property {
            ThemeStyleProperty::Display(val) => self.with_display(val),
            ThemeStyleProperty::PositionType(val) => self.with_position_type(val),
            ThemeStyleProperty::Direction(val) => self.with_direction(val),
            ThemeStyleProperty::FlexDirection(val) => self.with_flex_direction(val),
            ThemeStyleProperty::FlexWrap(val) => self.with_flex_wrap(val),
            ThemeStyleProperty::AlignItems(val) => self.with_align_items(val),
            ThemeStyleProperty::AlignSelf(val) => self.with_align_self(val),
            ThemeStyleProperty::AlignContent(val) => self.with_align_content(val),
            ThemeStyleProperty::JustifyContent(val) => self.with_justify_content(val),
            ThemeStyleProperty::Position(val) => self.with_position(val),
            ThemeStyleProperty::Margin(val) => self.with_margin(val),
            ThemeStyleProperty::Padding(val) => self.with_padding(val),
            ThemeStyleProperty::Border(val) => self.with_border(val),
            ThemeStyleProperty::FlexGrow(val) => self.with_flex_grow(val),
            ThemeStyleProperty::FlexShrink(val) => self.with_flex_shrink(val),
            ThemeStyleProperty::FlexBasis(val) => self.with_flex_basis(val),
            ThemeStyleProperty::Size(val) => self.with_size(val),
            ThemeStyleProperty::MinSize(val) => self.with_min_size(val),
            ThemeStyleProperty::MaxSize(val) => self.with_max_size(val),
            ThemeStyleProperty::AspectRatio(val) => self.with_aspect_ratio(val),
            ThemeStyleProperty::Overflow(val) => self.with_overflow(val),
        }
    }

    /// If this is set to [`Display::None`], this node will be collapsed.
    fn with_display(self, val: Display) -> Self {
        Self(Style {
            display: val,
            ..self.0
        })
    }

    /// Whether to arrange this node relative to other nodes, or positioned absolutely
    fn with_position_type(self, val: PositionType) -> Self {
        Self(Style {
            position_type: val,
            ..self.0
        })
    }

    /// Which direction the content of this node should go
    fn with_direction(self, val: Direction) -> Self {
        Self(Style {
            direction: val,
            ..self.0
        })
    }

    /// Whether to use column or row layout
    fn with_flex_direction(self, val: FlexDirection) -> Self {
        Self(Style {
            flex_direction: val,
            ..self.0
        })
    }

    /// How to wrap nodes
    fn with_flex_wrap(self, val: FlexWrap) -> Self {
        Self(Style {
            flex_wrap: val,
            ..self.0
        })
    }

    /// How items are aligned according to the cross axis
    fn with_align_items(self, val: AlignItems) -> Self {
        Self(Style {
            align_items: val,
            ..self.0
        })
    }

    /// Like align_items but for only this item
    fn with_align_self(self, val: AlignSelf) -> Self {
        Self(Style {
            align_self: val,
            ..self.0
        })
    }

    /// How to align each line, only applies if flex_wrap is set to
    /// [`FlexWrap::Wrap`] and there are multiple lines of items
    fn with_align_content(self, val: AlignContent) -> Self {
        Self(Style {
            align_content: val,
            ..self.0
        })
    }

    /// How items align according to the main axis
    fn with_justify_content(self, val: JustifyContent) -> Self {
        Self(Style {
            justify_content: val,
            ..self.0
        })
    }

    /// The position of the node as described by its Rect
    fn with_position(self, val: UiRect) -> Self {
        Self(Style {
            position: val,
            ..self.0
        })
    }

    /// The margin of the node
    fn with_margin(self, val: UiRect) -> Self {
        Self(Style {
            margin: val,
            ..self.0
        })
    }

    /// The padding of the node
    fn with_padding(self, val: UiRect) -> Self {
        Self(Style {
            padding: val,
            ..self.0
        })
    }

    /// The border of the node
    fn with_border(self, val: UiRect) -> Self {
        Self(Style {
            border: val,
            ..self.0
        })
    }

    /// Defines how much a flexbox item should grow if there's space available
    fn with_flex_grow(self, val: f32) -> Self {
        Self(Style {
            flex_grow: val,
            ..self.0
        })
    }

    /// How to shrink if there's not enough space available
    fn with_flex_shrink(self, val: f32) -> Self {
        Self(Style {
            flex_shrink: val,
            ..self.0
        })
    }

    /// The initial size of the item
    fn with_flex_basis(self, val: Val) -> Self {
        Self(Style {
            flex_basis: val,
            ..self.0
        })
    }

    /// The size of the flexbox
    fn with_size(self, val: Size) -> Self {
        Self(Style {
            size: val,
            ..self.0
        })
    }

    /// The minimum size of the flexbox
    fn with_min_size(self, val: Size) -> Self {
        Self(Style {
            min_size: val,
            ..self.0
        })
    }

    /// The maximum size of the flexbox
    fn with_max_size(self, val: Size) -> Self {
        Self(Style {
            max_size: val,
            ..self.0
        })
    }

    /// The aspect ratio of the flexbox
    fn with_aspect_ratio(self, val: Option<f32>) -> Self {
        Self(Style {
            aspect_ratio: val,
            ..self.0
        })
    }

    /// How to handle overflow
    fn with_overflow(self, val: Overflow) -> Self {
        Self(Style {
            overflow: val,
            ..self.0
        })
    }
}
