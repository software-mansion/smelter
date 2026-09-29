/// An NV12 frame stored in wgpu textures: either one [`wgpu::TextureFormat::NV12`] texture or
/// one texture per plane. [`VideoTexture::y_plane`] and [`VideoTexture::uv_plane`] work the same
/// way for both.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VideoTexture(Planes);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Planes {
    Single(wgpu::Texture),
    #[cfg_attr(vulkan, expect(dead_code))]
    Separated {
        y: wgpu::Texture,
        uv: wgpu::Texture,
    },
}

impl VideoTexture {
    /// Wraps a texture in [`wgpu::TextureFormat::NV12`].
    pub fn from_nv12_texture(texture: wgpu::Texture) -> Self {
        Self(Planes::Single(texture))
    }

    #[cfg_attr(vulkan, expect(dead_code))]
    pub(crate) fn from_planes(y_texture: wgpu::Texture, uv_texture: wgpu::Texture) -> Self {
        Self(Planes::Separated {
            y: y_texture,
            uv: uv_texture,
        })
    }

    /// The whole frame as one NV12 texture, if it is stored that way.
    pub fn nv12_texture(&self) -> Option<&wgpu::Texture> {
        match &self.0 {
            Planes::Single(texture) => Some(texture),
            Planes::Separated { .. } => None,
        }
    }

    /// Size of the frame in luma samples.
    pub fn size(&self) -> wgpu::Extent3d {
        match &self.0 {
            Planes::Single(texture) => texture.size(),
            Planes::Separated { y, .. } => y.size(),
        }
    }

    pub fn width(&self) -> u32 {
        self.size().width
    }

    pub fn height(&self) -> u32 {
        self.size().height
    }

    pub fn usage(&self) -> wgpu::TextureUsages {
        match &self.0 {
            Planes::Single(texture) => texture.usage(),
            Planes::Separated { y, .. } => y.usage(),
        }
    }

    pub fn y_plane(&self) -> VideoTexturePlane<'_> {
        match &self.0 {
            Planes::Single(texture) => VideoTexturePlane {
                texture,
                aspect: wgpu::TextureAspect::Plane0,
                format: wgpu::TextureFormat::R8Unorm,
                size: texture.size(),
            },
            Planes::Separated { y, .. } => VideoTexturePlane {
                texture: y,
                aspect: wgpu::TextureAspect::All,
                format: y.format(),
                size: y.size(),
            },
        }
    }

    pub fn uv_plane(&self) -> VideoTexturePlane<'_> {
        match &self.0 {
            Planes::Single(texture) => {
                let size = texture.size();
                VideoTexturePlane {
                    texture,
                    aspect: wgpu::TextureAspect::Plane1,
                    format: wgpu::TextureFormat::Rg8Unorm,
                    size: wgpu::Extent3d {
                        width: size.width / 2,
                        height: size.height / 2,
                        depth_or_array_layers: size.depth_or_array_layers,
                    },
                }
            }
            Planes::Separated { uv, .. } => VideoTexturePlane {
                texture: uv,
                aspect: wgpu::TextureAspect::All,
                format: uv.format(),
                size: uv.size(),
            },
        }
    }

    pub fn planes(&self) -> [VideoTexturePlane<'_>; 2] {
        [self.y_plane(), self.uv_plane()]
    }

    /// Records a copy of both planes into `destination`, which needs the same size.
    /// wgpu cannot copy from separate plane textures into an NV12 texture.
    pub fn copy_to(&self, command_encoder: &mut wgpu::CommandEncoder, destination: &VideoTexture) {
        for (source_plane, destination_plane) in self.planes().into_iter().zip(destination.planes())
        {
            command_encoder.copy_texture_to_texture(
                source_plane.as_image_copy(),
                destination_plane.as_image_copy(),
                source_plane.size(),
            );
        }
    }
}

/// One plane of a [`VideoTexture`]: the texture, the aspect selecting the plane in it, and the
/// plane's format and size.
#[derive(Debug, Clone, Copy)]
pub struct VideoTexturePlane<'a> {
    texture: &'a wgpu::Texture,
    aspect: wgpu::TextureAspect,
    format: wgpu::TextureFormat,
    size: wgpu::Extent3d,
}

impl<'a> VideoTexturePlane<'a> {
    /// The texture holding this plane. Use it together with [`VideoTexturePlane::aspect`].
    pub fn texture(&self) -> &'a wgpu::Texture {
        self.texture
    }

    pub fn aspect(&self) -> wgpu::TextureAspect {
        self.aspect
    }

    /// [`wgpu::TextureFormat::R8Unorm`] for Y, [`wgpu::TextureFormat::Rg8Unorm`] for UV.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Size in the plane's own texels, so UV is half the frame size.
    pub fn size(&self) -> wgpu::Extent3d {
        self.size
    }

    pub fn create_view(&self, label: Option<&str>) -> wgpu::TextureView {
        self.create_view_with_usage(label, None)
    }

    pub fn create_view_with_usage(
        &self,
        label: Option<&str>,
        usage: Option<wgpu::TextureUsages>,
    ) -> wgpu::TextureView {
        self.texture.create_view(&wgpu::TextureViewDescriptor {
            label,
            format: Some(self.format),
            dimension: Some(wgpu::TextureViewDimension::D2),
            usage,
            aspect: self.aspect,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        })
    }

    /// Like [`wgpu::Texture::as_image_copy`], with the plane's aspect.
    pub fn as_image_copy(&self) -> wgpu::TexelCopyTextureInfo<'a> {
        wgpu::TexelCopyTextureInfo {
            texture: self.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: self.aspect,
        }
    }
}
