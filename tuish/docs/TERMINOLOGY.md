# Tuish Terminology

## Core Concepts

### Hierarchy
```
AppUI
├── FocusedArea: ContentArea | CommandInput
│   
├── CommandInput Area (singleton)
│   └── Text input widget at bottom
│
└── ContentArea
    └── LayoutManager (tree structure)
        └── Region(s) [LayoutNode::Tabs]
            └── Pane(s) [identified by PaneId]
```

### Terms

**Region**: A content area that can host multiple panes via tabs. This is a `LayoutNode::Tabs` in the layout tree. Regions can be split horizontally or vertically to create multiple side-by-side regions.

**Pane**: Individual content like Terminal, Environment, History, Aliases, Functions, CallStack. Each pane has a unique `PaneId`.

**ContentArea**: The main display area (as opposed to CommandInput). Contains the layout tree with one or more regions.

**CommandInput**: The text input area at the bottom of the screen where commands are typed.

**FocusedArea**: Enum tracking whether ContentArea or CommandInput currently has focus.

**Layout Tree**: Hierarchical structure of:
- `Tabs` nodes (regions/leaf nodes containing panes)
- `HSplit` nodes (horizontal split: left | right)
- `VSplit` nodes (vertical split: top / bottom)

## Navigation Model

### Focus Levels
1. **Area Level**: ContentArea vs CommandInput (Ctrl+Space)
2. **Region Level**: Which region when multiple splits exist (Ctrl+B n/p)
3. **Tab Level**: Which pane within a region (Ctrl+Tab/Shift+Tab)

### Hotkeys (Current)
- **Ctrl+Space**: Toggle between ContentArea and CommandInput
- **Ctrl+Tab**: Next tab in focused region
- **Ctrl+Shift+Tab**: Previous tab in focused region
- **Ctrl+B**: Enter navigation mode
  - **n**: Next region
  - **p**: Previous region
  - **Tab**: Cycle tabs in region
  - **Esc**: Exit navigation mode

### Hotkeys (Planned)
- **Ctrl+B + letter**: Jump to specific pane type
  - **Ctrl+E**: Environment
  - **Ctrl+H**: History
  - **Ctrl+A**: Aliases
  - **Ctrl+F**: Functions
  - **Ctrl+C**: CallStack
  - **Ctrl+T**: Terminal (if multiple)
- **Ctrl+B v**: Vertical split
- **Ctrl+B h**: Horizontal split
- **Ctrl+B x**: Close region
- Future: Move panes between regions

## Visual Feedback

### Border Colors (Three States)
1. **Bright** (e.g., purple): ContentArea focused AND this is the active region
2. **Medium** (half brightness): ContentArea focused but different region
3. **Dim** (dark gray): CommandInput focused

### Tab Styling
- **Selected tab**: Bright background with icon
- **Unselected tabs**: Dimmed background
- Tabs only visible when region has multiple panes
