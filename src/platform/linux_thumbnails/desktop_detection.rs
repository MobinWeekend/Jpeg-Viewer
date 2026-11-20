//! Lightweight desktop-environment detection used only to choose a preferred
//! thumbnail-generation backend. Cache lookup itself is desktop-independent.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Desktop {
    GnomeCompatible,
    Kde,
    Xfce,
    Cosmic,
    Other,
}

pub(super) fn detect() -> Desktop {
    let current = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let session = std::env::var("XDG_SESSION_DESKTOP").unwrap_or_default();
    detect_from_values(&current, &session)
}

fn detect_from_values(current: &str, session: &str) -> Desktop {
    let values = current
        .split([':', ';'])
        .chain(session.split([':', ';']))
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    // Check the most specific names first. Some sessions report a composite
    // value such as "Budgie:GNOME" or "pop:GNOME".
    if values.iter().any(|value| value.contains("cosmic")) {
        return Desktop::Cosmic;
    }
    if values
        .iter()
        .any(|value| value.contains("kde") || value.contains("plasma"))
    {
        return Desktop::Kde;
    }
    if values.iter().any(|value| value.contains("xfce")) {
        return Desktop::Xfce;
    }
    if values.iter().any(|value| {
        value.contains("gnome")
            || value.contains("cinnamon")
            || value.contains("mate")
            || value.contains("budgie")
    }) {
        return Desktop::GnomeCompatible;
    }

    Desktop::Other
}

#[cfg(test)]
mod tests {
    use super::{Desktop, detect_from_values};

    #[test]
    fn recognizes_requested_desktops() {
        assert_eq!(detect_from_values("COSMIC", "cosmic"), Desktop::Cosmic);
        assert_eq!(
            detect_from_values("GNOME", "gnome"),
            Desktop::GnomeCompatible
        );
        assert_eq!(detect_from_values("KDE", "plasma"), Desktop::Kde);
        assert_eq!(detect_from_values("XFCE", "xfce"), Desktop::Xfce);
        assert_eq!(
            detect_from_values("X-Cinnamon", ""),
            Desktop::GnomeCompatible
        );
        assert_eq!(detect_from_values("MATE", "mate"), Desktop::GnomeCompatible);
        assert_eq!(
            detect_from_values("Budgie:GNOME", "budgie"),
            Desktop::GnomeCompatible
        );
    }
}
