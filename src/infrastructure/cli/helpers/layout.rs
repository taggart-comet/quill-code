use ratatui::layout::Rect;

pub fn centered_rect(size: Rect, width: u16, height: u16) -> Rect {
    let x = size.x + (size.width.saturating_sub(width)) / 2;
    let y = size.y + (size.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

pub fn bottom_left_aligned_rect(size: Rect, width: u16, height: u16) -> Rect {
    let x = size.x;
    let y = size
        .y
        .saturating_add(size.height.saturating_sub(height + 3));
    Rect {
        x,
        y,
        width,
        height,
    }
}
