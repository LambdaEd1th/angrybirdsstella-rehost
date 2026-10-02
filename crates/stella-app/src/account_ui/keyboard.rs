//! Window-independent account keys shared by desktop and browser hosts.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NamedKey {
    Escape,
    Tab,
    Enter,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    Backspace,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Key<'a> {
    Named(NamedKey),
    Character(&'a str),
    Other,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct ModifiersState(u8);

impl ModifiersState {
    pub(crate) const SHIFT: Self = Self(1);
    pub(crate) const CONTROL: Self = Self(2);
    pub(crate) const SUPER: Self = Self(4);

    pub(crate) const fn empty() -> Self {
        Self(0)
    }

    pub(crate) fn new(shift: bool, control: bool, super_key: bool) -> Self {
        let mut value = Self::empty();
        value.0 = u8::from(shift) | (u8::from(control) << 1) | (u8::from(super_key) << 2);
        value
    }

    pub(crate) fn shift_key(self) -> bool {
        self.0 & Self::SHIFT.0 != 0
    }

    pub(crate) fn control_key(self) -> bool {
        self.0 & Self::CONTROL.0 != 0
    }

    pub(crate) fn super_key(self) -> bool {
        self.0 & Self::SUPER.0 != 0
    }
}
