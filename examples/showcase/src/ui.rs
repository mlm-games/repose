use repose_core::prelude::*;
use repose_ui::*;

pub fn Card() -> View {
    Box(Modifier::new()
        .background(repose_core::theme().surface)
        .border(1.0, Color::from_hex("#333333"), 8.0)
        .padding(16.0))
}

pub fn Section(title: &str, body: View) -> View {
    Column(Modifier::new().padding(8.0)).child((
        TextSize(
            TextColor(Text(title), repose_core::theme().on_surface),
            18.0,
        )
        .modifier(Modifier::new().padding(4.0)),
        Card().child(body),
    ))
}

pub fn TopBar() -> View {
    Row(Modifier::new()
        .padding(12.0)
        .background(Color::from_hex("#1E1E1E")))
}

pub fn Tabs(items: &[(&str, super::Page)], current: repose_core::Signal<super::Page>) -> View {
    Row(Modifier::new().padding(8.0)).child(
        items
            .iter()
            .map(|(label, p)| {
                let cur = current.get();
                let selected = *p == cur;
                let col = if selected {
                    repose_core::theme().primary
                } else {
                    Color::from_hex("#2A2A2A")
                };
                Button(*label, {
                    let current = current.clone();
                    let p = *p;
                    move || current.set(p)
                })
                .modifier(
                    Modifier::new()
                        .padding(4.0)
                        .background(col)
                        .clip_rounded(6.0),
                )
            })
            .collect::<Vec<_>>(),
    )
}
