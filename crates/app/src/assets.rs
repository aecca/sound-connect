//! Icons the panel uses (Lucide, ISC license), on top of GPUI Kit's own.

use std::borrow::Cow;

use gpui_kit::*;

const ICONS: [(&str, &[u8]); 4] = [
    ("icons/ear.svg", include_bytes!("../assets/icons/ear.svg")),
    ("icons/ear-off.svg", include_bytes!("../assets/icons/ear-off.svg")),
    ("icons/circle-off.svg", include_bytes!("../assets/icons/circle-off.svg")),
    ("icons/headphones.svg", include_bytes!("../assets/icons/headphones.svg")),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match ICONS.iter().find(|(name, _)| *name == path) {
            Some((_, bytes)) => Ok(Some(Cow::Borrowed(bytes))),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut names = gpui_kit::assets::Assets.list(path)?;
        names.extend(ICONS.iter().filter(|(name, _)| name.starts_with(path)).map(|(name, _)| (*name).into()));
        Ok(names)
    }
}
