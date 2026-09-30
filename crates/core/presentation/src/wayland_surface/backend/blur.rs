//! Compositor blur behind a surface.
//!
//! Two protocols carry the same model — the client names surface-local
//! rectangles, the compositor blurs whatever is behind them:
//! `ext-background-effect-v1` (staging; Hyprland, KWin, niri) and its KDE
//! predecessor `org_kde_kwin_blur`. The ext protocol is preferred while it
//! advertises its `blur` capability. The capability arrives as an event after
//! binding and may be withdrawn, so the choice is re-read on every staging.

use super::*;

/// Which protocol realizes a surface's blur region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wayland_surface) enum BlurProtocol {
    BackgroundEffect,
    Kde,
}

/// The blur-protocol globals bound on this connection.
#[derive(Default)]
pub(in crate::wayland_surface) struct BlurProtocols {
    pub(in crate::wayland_surface) background_effect: Option<ExtBackgroundEffectManagerV1>,
    /// Last `capabilities` event from `background_effect` had the `blur` bit.
    pub(in crate::wayland_surface) background_effect_blur: bool,
    pub(in crate::wayland_surface) kde: Option<OrgKdeKwinBlurManager>,
}

impl BlurProtocols {
    pub(in crate::wayland_surface) fn preferred(&self) -> Option<BlurProtocol> {
        preferred_blur_protocol(
            self.background_effect.is_some(),
            self.background_effect_blur,
            self.kde.is_some(),
        )
    }
}

pub(in crate::wayland_surface) fn preferred_blur_protocol(
    background_effect_bound: bool,
    background_effect_blur: bool,
    kde_bound: bool,
) -> Option<BlurProtocol> {
    if background_effect_bound && background_effect_blur {
        Some(BlurProtocol::BackgroundEffect)
    } else if kde_bound {
        Some(BlurProtocol::Kde)
    } else {
        None
    }
}

/// The per-surface object of whichever protocol is in use.
pub(in crate::wayland_surface) enum SurfaceBlurObject {
    BackgroundEffect(ExtBackgroundEffectSurfaceV1),
    Kde(OrgKdeKwinBlur),
}

impl SurfaceBlurObject {
    fn protocol(&self) -> BlurProtocol {
        match self {
            Self::BackgroundEffect(_) => BlurProtocol::BackgroundEffect,
            Self::Kde(_) => BlurProtocol::Kde,
        }
    }

    /// Remove the effect and destroy the object; takes effect on the next
    /// surface commit. A null KDE region means whole-surface blur, so KDE blur
    /// is cleared by `unset` rather than by an empty region.
    pub(in crate::wayland_surface) fn release(
        self,
        protocols: &BlurProtocols,
        wl_surface: &wl_surface::WlSurface,
    ) {
        match self {
            Self::BackgroundEffect(effect) => effect.destroy(),
            Self::Kde(blur) => {
                if let Some(manager) = protocols.kde.as_ref() {
                    manager.unset(wl_surface);
                }
                blur.release();
            }
        }
    }

    /// Destroy the object without touching the surface, which is itself
    /// being destroyed.
    pub(in crate::wayland_surface) fn destroy(&self) {
        match self {
            Self::BackgroundEffect(effect) => effect.destroy(),
            Self::Kde(blur) => blur.release(),
        }
    }
}

/// Stage the entry's pending blur regions for its next commit. Returns
/// whether protocol state was staged, i.e. whether that commit carries it.
pub(in crate::wayland_surface) fn stage_blur_region(
    protocols: &BlurProtocols,
    compositor_state: &CompositorState,
    entry: &mut SurfaceEntry,
    qh: &QueueHandle<State>,
) -> bool {
    let preferred = protocols.preferred();
    let mut staged = false;
    // The protocol in use changed (the ext `blur` capability arrived or was
    // withdrawn): drop the old object and restage on the new one.
    if entry
        .blur_object
        .as_ref()
        .is_some_and(|object| Some(object.protocol()) != preferred)
    {
        let wl_surface = entry.wl_surface().clone();
        if let Some(object) = entry.blur_object.take() {
            object.release(protocols, &wl_surface);
        }
        entry.blur_committed = false;
        entry.blur_region_dirty = true;
        staged = true;
    }
    if !entry.blur_region_dirty {
        return staged;
    }
    if !entry.blur_regions.is_empty() {
        let Some(protocol) = preferred else {
            // No protocol can apply this hint. Treat it as an intentional
            // no-op rather than retrying forever; a protocol that appears later
            // marks every surface dirty again.
            entry.blur_region_dirty = false;
            return staged;
        };
        let Ok(region) = Region::new(compositor_state) else {
            return staged;
        };
        for rect in &entry.blur_regions {
            region.add(
                rect.x as i32,
                rect.y as i32,
                rect.width as i32,
                rect.height as i32,
            );
        }
        // Created lazily, the first time this surface actually needs blur: a
        // KDE blur object with no region blurs the whole surface.
        let wl_surface = entry.wl_surface().clone();
        let object = entry.blur_object.get_or_insert_with(|| match protocol {
            BlurProtocol::BackgroundEffect => SurfaceBlurObject::BackgroundEffect(
                protocols
                    .background_effect
                    .as_ref()
                    .expect("preferred protocol is bound")
                    .get_background_effect(&wl_surface, qh, ()),
            ),
            BlurProtocol::Kde => SurfaceBlurObject::Kde(
                protocols
                    .kde
                    .as_ref()
                    .expect("preferred protocol is bound")
                    .create(&wl_surface, qh, ()),
            ),
        });
        match object {
            SurfaceBlurObject::BackgroundEffect(effect) => {
                effect.set_blur_region(Some(region.wl_region()));
            }
            SurfaceBlurObject::Kde(blur) => {
                blur.set_region(Some(region.wl_region()));
                blur.commit();
            }
        }
        entry.blur_committed = true;
        return true;
    }

    if let Some(object) = entry.blur_object.take() {
        let wl_surface = entry.wl_surface().clone();
        object.release(protocols, &wl_surface);
        entry.blur_committed = false;
        return true;
    }
    entry.blur_committed = false;
    entry.blur_region_dirty = false;
    staged
}
