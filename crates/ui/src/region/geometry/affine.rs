//! Affine composition for inherited region presentation.

use super::UiScreenRect;

#[derive(Clone, Copy)]
pub(super) struct Affine2 {
    pub(super) xx: f64,
    pub(super) xy: f64,
    pub(super) yx: f64,
    pub(super) yy: f64,
    pub(super) tx: f64,
    pub(super) ty: f64,
}

impl Affine2 {
    pub(super) const IDENTITY: Self = Self {
        xx: 1.0,
        xy: 0.0,
        yx: 0.0,
        yy: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub(super) fn scale_about(scale: f64, center_x: f64, center_y: f64) -> Self {
        Self {
            xx: scale,
            xy: 0.0,
            yx: 0.0,
            yy: scale,
            tx: center_x * (1.0 - scale),
            ty: center_y * (1.0 - scale),
        }
    }

    pub(super) const fn translation(x: f64, y: f64) -> Self {
        Self {
            tx: x,
            ty: y,
            ..Self::IDENTITY
        }
    }

    pub(super) fn compose(self, local: Self) -> Self {
        Self {
            xx: self.xx * local.xx + self.xy * local.yx,
            xy: self.xx * local.xy + self.xy * local.yy,
            yx: self.yx * local.xx + self.yy * local.yx,
            yy: self.yx * local.xy + self.yy * local.yy,
            tx: self.xx * local.tx + self.xy * local.ty + self.tx,
            ty: self.yx * local.tx + self.yy * local.ty + self.ty,
        }
    }

    pub(super) fn transform(self, x: f64, y: f64) -> (f64, f64) {
        (
            self.xx * x + self.xy * y + self.tx,
            self.yx * x + self.yy * y + self.ty,
        )
    }

    pub(super) fn bounds(self, rect: UiScreenRect) -> UiScreenRect {
        let corners = [
            self.transform(rect.left, rect.bottom),
            self.transform(rect.left, rect.top),
            self.transform(rect.right, rect.bottom),
            self.transform(rect.right, rect.top),
        ];
        UiScreenRect {
            left: corners
                .iter()
                .map(|corner| corner.0)
                .fold(f64::INFINITY, f64::min),
            bottom: corners
                .iter()
                .map(|corner| corner.1)
                .fold(f64::INFINITY, f64::min),
            right: corners
                .iter()
                .map(|corner| corner.0)
                .fold(f64::NEG_INFINITY, f64::max),
            top: corners
                .iter()
                .map(|corner| corner.1)
                .fold(f64::NEG_INFINITY, f64::max),
        }
    }
}
