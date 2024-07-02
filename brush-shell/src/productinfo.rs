//! Information about this shell project.

/// The formal name of this product.
pub const PRODUCT_NAME: &str = "brush";

const PRODUCT_HOMEPAGE: &str = env!("CARGO_PKG_HOMEPAGE");
const PRODUCT_REPO: &str = env!("CARGO_PKG_REPOSITORY");

/// The URI to display as the product's homepage.
#[allow(clippy::const_is_empty)]
pub const PRODUCT_DISPLAY_URI: &str = if !PRODUCT_HOMEPAGE.is_empty() {
    PRODUCT_HOMEPAGE
} else {
    PRODUCT_REPO
};

/// The version of the product, in string form.
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Info regarding the specific version of sources used to build this product.
pub const PRODUCT_GIT_VERSION: &str = git_version::git_version!();

pub(crate) fn get_product_display_str() -> String {
    std::format!(
        "{PRODUCT_NAME} version {PRODUCT_VERSION} ({PRODUCT_GIT_VERSION}) - {PRODUCT_DISPLAY_URI}"
    )
}
