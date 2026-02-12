use bollard::Docker;
use futures_util::StreamExt;
use tracing::{debug, info};

use crate::error::{ComposeError, Result};

/// Check if an image exists locally.
pub async fn image_exists(docker: &Docker, image: &str) -> bool {
    docker.inspect_image(image).await.is_ok()
}

/// Pull an image from a registry. Streams progress and returns when complete.
pub async fn pull_image(docker: &Docker, image: &str) -> Result<()> {
    info!(image = %image, "pulling image");

    let (image_name, tag) = parse_image_ref(image);

    let options = bollard::query_parameters::CreateImageOptions {
        from_image: Some(image_name.to_string()),
        tag: Some(tag.to_string()),
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(options), None, None);

    while let Some(result) = stream.next().await {
        match result {
            Ok(info) => {
                if let Some(status) = &info.status {
                    debug!(image = %image, status = %status, "pull progress");
                }
                if let Some(err) = &info.error_detail {
                    return Err(ComposeError::ImagePullError {
                        image: image.to_string(),
                        reason: format!("{:?}", err),
                    });
                }
            }
            Err(e) => {
                return Err(ComposeError::ImagePullError {
                    image: image.to_string(),
                    reason: e.to_string(),
                });
            }
        }
    }

    info!(image = %image, "image pulled successfully");
    Ok(())
}

/// Pull all missing images concurrently.
pub async fn ensure_images(docker: &Docker, images: &[String]) -> Result<()> {
    // Deduplicate
    let mut unique: Vec<String> = images.to_vec();
    unique.sort();
    unique.dedup();

    let mut handles = Vec::new();

    for image in unique {
        let docker = docker.clone();
        let img = image.clone();
        handles.push(tokio::spawn(async move {
            if !image_exists(&docker, &img).await {
                pull_image(&docker, &img).await
            } else {
                debug!(image = %img, "image already exists locally");
                Ok(())
            }
        }));
    }

    for handle in handles {
        handle
            .await
            .map_err(|e| ComposeError::ImagePullError {
                image: "unknown".to_string(),
                reason: format!("task join error: {e}"),
            })??;
    }

    Ok(())
}

/// Parse an image reference into (name, tag). Defaults tag to "latest".
fn parse_image_ref(image: &str) -> (&str, &str) {
    // Handle digests: image@sha256:...
    if image.contains('@') {
        return (image, "");
    }

    // Handle tags: image:tag, registry/image:tag
    // Be careful with registry URLs like registry.example.com:5000/image:tag
    if let Some(colon_pos) = image.rfind(':') {
        let after_colon = &image[colon_pos + 1..];
        // If after colon contains '/', it's a registry port, not a tag
        if !after_colon.contains('/') {
            return (&image[..colon_pos], after_colon);
        }
    }

    (image, "latest")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_image_ref_with_tag() {
        assert_eq!(parse_image_ref("nginx:1.25"), ("nginx", "1.25"));
    }

    #[test]
    fn test_parse_image_ref_latest() {
        assert_eq!(parse_image_ref("nginx"), ("nginx", "latest"));
    }

    #[test]
    fn test_parse_image_ref_with_registry() {
        assert_eq!(
            parse_image_ref("registry.example.com/myapp:v2"),
            ("registry.example.com/myapp", "v2")
        );
    }

    #[test]
    fn test_parse_image_ref_registry_with_port() {
        assert_eq!(
            parse_image_ref("registry.example.com:5000/myapp:v2"),
            ("registry.example.com:5000/myapp", "v2")
        );
    }

    #[test]
    fn test_parse_image_ref_registry_with_port_no_tag() {
        assert_eq!(
            parse_image_ref("registry.example.com:5000/myapp"),
            ("registry.example.com:5000/myapp", "latest")
        );
    }

    #[test]
    fn test_parse_image_ref_digest() {
        let img = "nginx@sha256:abc123";
        assert_eq!(parse_image_ref(img), (img, ""));
    }
}
