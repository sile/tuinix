#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalText {
    // TODO: private
    pub content: String,
    pub style: TerminalStyle,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalStyle {}
