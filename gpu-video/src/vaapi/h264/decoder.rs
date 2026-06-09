#[cfg(target_os = "linux")]
mod imp {
    use std::{collections::HashMap, rc::Rc, sync::Arc};

    use crate::{
        EncodedInputChunk, H264DecoderEvent, OutputFrame, VideoResolution,
        codec::h264::parameters::h264_max_num_reorder_frames,
        device::{ColorRange, ColorSpace, MissedFrameHandling},
        parser::{
            decoder_instructions::{DecoderInstruction, compile_to_decoder_instructions},
            h264::H264Parser,
            reference_manager::{
                DecodeInformation, ReferenceContext, ReferenceId,
                ReferenceManagementError, ReferencePictureInfo,
            },
        },
        vaapi::display::{
            export_surface_as_frame, invalid_h264_pictures, open_display,
            take_nv12_surface,
        },
        vulkan_decoder::{DecodeResult, DecodeResultMetadata, FrameSorter},
    };
    use h264_reader::nal::{
        pps::PicParameterSet,
        slice::{
            DecRefPicMarking, FieldPic, MemoryManagementControlOperation,
            NumRefIdxActive, PredWeightTable, SliceFamily, SliceHeader,
        },
        sps::{
            ChromaFormat, FrameMbsFlags, PicOrderCntType, Profile, ScalingList,
            SeqParameterSet,
        },
    };
    use libva::{
        BufferType, Config, Context, Display, H264PicFields, H264SeqFields, IQMatrix,
        IQMatrixBufferH264, Picture, PictureH264, PictureNew, PictureParameter,
        PictureParameterBufferH264, SliceParameter, SliceParameterBufferH264, Surface,
        UsageHint, VA_PICTURE_H264_LONG_TERM_REFERENCE,
        VA_PICTURE_H264_SHORT_TERM_REFERENCE, VA_RT_FORMAT_YUV420,
        VA_SLICE_DATA_FLAG_ALL, VAConfigAttrib, VAConfigAttribType, VAEntrypoint,
        VAProfile,
    };
    use tracing::info;

    const DECODE_SURFACE_ALLOCATION_BATCH: usize = 4;

    pub struct H264Decoder {
        display: Rc<Display>,
        session: Option<VaapiDecodeSession>,
        parser: H264Parser,
        reference_ctx: ReferenceContext,
        references: Vec<(ReferenceId, libva::VASurfaceID)>,
        active_surfaces: HashMap<libva::VASurfaceID, DecodedSurface>,
        free_surfaces: Vec<Surface<()>>,
        frame_sorter: FrameSorter<DecodedFrameData>,
        sps: HashMap<u8, SeqParameterSet>,
        pps: HashMap<u8, PicParameterSet>,
    }

    pub struct WgpuTexturesDecoder {
        decoder: H264Decoder,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    }

    struct DecodedFrameData {
        surface_id: libva::VASurfaceID,
        resolution: VideoResolution,
    }

    #[derive(Debug, thiserror::Error)]
    pub enum VaapiH264DecoderError {
        #[error("VA-API H264 decoder is unavailable: {0}")]
        Unavailable(String),

        #[error("VA-API H264 decode error: {0}")]
        Decode(String),
    }

    struct DecodedSurface {
        surface: Surface<()>,
        reference: Option<DecodedPictureInfo>,
        output_pending: bool,
        reusable: bool,
    }

    impl DecodedSurface {
        fn new(surface: Surface<()>, reference: DecodedPictureInfo) -> Self {
            Self {
                surface,
                reference: Some(reference),
                output_pending: true,
                reusable: true,
            }
        }

        fn clear_reference(&mut self) {
            self.reference = None;
        }

        fn release_output(&mut self) {
            self.output_pending = false;
        }

        fn retire_session(&mut self, reusable: bool) {
            self.reusable &= reusable;
            self.clear_reference();
        }

        fn is_recyclable(&self) -> bool {
            self.reference.is_none() && !self.output_pending
        }
    }

    impl H264Decoder {
        fn new(
            adapter_info: Option<&wgpu::AdapterInfo>,
        ) -> Result<Self, VaapiH264DecoderError> {
            info!("Initializing VA-API H264 decoder");
            let display =
                open_display(adapter_info).map_err(VaapiH264DecoderError::Unavailable)?;
            Ok(Self {
                display,
                session: None,
                parser: H264Parser::default(),
                reference_ctx: ReferenceContext::new(MissedFrameHandling::Strict),
                references: Vec::new(),
                active_surfaces: HashMap::new(),
                free_surfaces: Vec::new(),
                frame_sorter: FrameSorter::new(),
                sps: HashMap::new(),
                pps: HashMap::new(),
            })
        }

        fn process_event(
            &mut self,
            event: H264DecoderEvent<'_>,
        ) -> Result<Vec<OutputFrame<DecodedFrameData>>, VaapiH264DecoderError> {
            match event {
                H264DecoderEvent::DecodeChunk(chunk) => {
                    let instructions = self
                        .parse_h264(chunk.data, chunk.pts)
                        .map_err(VaapiH264DecoderError::Decode)?;
                    self.process_instructions(instructions)
                        .map_err(VaapiH264DecoderError::Decode)
                }
                H264DecoderEvent::DecodeParsedFrame(access_unit) => {
                    let instructions = compile_to_decoder_instructions(
                        &mut self.reference_ctx,
                        vec![access_unit],
                    )
                    .map_err(|err: ReferenceManagementError| {
                        VaapiH264DecoderError::Decode(err.to_string())
                    })?;
                    self.process_instructions(instructions)
                        .map_err(VaapiH264DecoderError::Decode)
                }
                H264DecoderEvent::SignalFrameEnd => self.signal_frame_end(),
                H264DecoderEvent::SignalDataLoss => {
                    self.signal_data_loss();
                    Ok(Vec::new())
                }
                H264DecoderEvent::Flush => self.flush(),
            }
        }

        fn signal_frame_end(
            &mut self,
        ) -> Result<Vec<OutputFrame<DecodedFrameData>>, VaapiH264DecoderError> {
            let instructions =
                self.flush_parser().map_err(VaapiH264DecoderError::Decode)?;
            self.process_instructions(instructions).map_err(VaapiH264DecoderError::Decode)
        }

        fn flush(
            &mut self,
        ) -> Result<Vec<OutputFrame<DecodedFrameData>>, VaapiH264DecoderError> {
            let mut frames = self.signal_frame_end()?;
            frames.extend(self.flush_sorted_frames());
            Ok(frames)
        }

        fn signal_data_loss(&mut self) {
            self.discard_sorted_frames();
            self.reference_ctx.mark_missed_frames();
        }

        fn parse_h264(
            &mut self,
            data: &[u8],
            pts: Option<u64>,
        ) -> Result<Vec<DecoderInstruction>, String> {
            let access_units =
                self.parser.parse(data, pts).map_err(|err| err.to_string())?;
            compile_to_decoder_instructions(&mut self.reference_ctx, access_units)
                .map_err(|err: ReferenceManagementError| err.to_string())
        }

        fn flush_parser(&mut self) -> Result<Vec<DecoderInstruction>, String> {
            let access_units = self.parser.flush().map_err(|err| err.to_string())?;
            compile_to_decoder_instructions(&mut self.reference_ctx, access_units)
                .map_err(|err: ReferenceManagementError| err.to_string())
        }

        fn process_instructions(
            &mut self,
            instructions: Vec<DecoderInstruction>,
        ) -> Result<Vec<OutputFrame<DecodedFrameData>>, String> {
            let mut frames = Vec::new();
            for instruction in instructions {
                match instruction {
                    DecoderInstruction::Sps(sps) => {
                        frames.extend(self.process_sps(sps)?);
                    }
                    DecoderInstruction::Pps(pps) => {
                        self.pps.insert(pps.pic_parameter_set_id.id(), pps);
                    }
                    DecoderInstruction::Idr { decode_info, reference_id } => {
                        self.retain_references()?;
                        if let Some(frame) =
                            self.decode_picture(decode_info, reference_id, true)?
                        {
                            frames.extend(self.sort_frame(frame));
                        }
                    }
                    DecoderInstruction::Decode { decode_info, reference_id } => {
                        if let Some(frame) =
                            self.decode_picture(decode_info, reference_id, false)?
                        {
                            frames.extend(self.sort_frame(frame));
                        }
                    }
                    DecoderInstruction::Drop { reference_ids } => {
                        for reference_id in reference_ids {
                            self.drop_reference(reference_id)?;
                        }
                    }
                }
            }
            Ok(frames)
        }

        fn process_sps(
            &mut self,
            sps: SeqParameterSet,
        ) -> Result<Vec<OutputFrame<DecodedFrameData>>, String> {
            let stream = VaapiStreamInfo::from_sps(&sps)?;
            let mut frames = Vec::new();
            if self.session.as_ref().is_none_or(|session| session.stream != stream) {
                frames.extend(self.flush_sorted_frames());
                self.retire_session_surfaces(false);
                self.session = Some(VaapiDecodeSession::new(&self.display, stream)?);
            }
            self.sps.insert(sps.id().id(), sps);
            Ok(frames)
        }

        fn decode_picture(
            &mut self,
            decode_info: DecodeInformation,
            reference_id: ReferenceId,
            is_idr: bool,
        ) -> Result<Option<DecodeResult<DecodedFrameData>>, String> {
            let session = self
                .session
                .as_ref()
                .ok_or_else(|| "missing VA-API decode session".to_string())?;
            let context = Rc::clone(&session.context);
            let coded_resolution = session.stream.coded_resolution;
            let display_resolution = session.stream.display_resolution;
            let max_num_reorder_frames = session.stream.max_num_reorder_frames;
            let pts = decode_info.pts;
            let pic_order_cnt = decode_info.picture_info.PicOrderCnt_for_decoding[0];
            let decoded_picture = DecodedPictureInfo::from_decode_info(&decode_info);

            let surface = self.take_surface(coded_resolution)?;
            let sps = self
                .sps
                .get(&decode_info.sps_id)
                .ok_or_else(|| format!("unknown SPS id {}", decode_info.sps_id))?;
            let color_space = ColorSpace::from(sps);
            let color_range = ColorRange::from(sps);
            let pps = self
                .pps
                .get(&decode_info.pps_id)
                .ok_or_else(|| format!("unknown PPS id {}", decode_info.pps_id))?;
            let surface_id = surface.id();
            let mut picture =
                Picture::new(pts.unwrap_or_default(), Rc::clone(&context), surface);
            self.add_buffers(&mut picture, &context, surface_id, decode_info, sps, pps)?;

            let picture = picture
                .begin()
                .map_err(|err| format!("failed to begin VA-API picture: {err}"))?
                .render()
                .map_err(|err| format!("failed to render VA-API picture: {err}"))?
                .end()
                .map_err(|err| format!("failed to end VA-API picture: {err}"))?
                .sync()
                .map_err(|(err, _)| format!("failed to sync VA-API picture: {err}"))?;
            let surface = picture
                .take_surface()
                .map_err(|_| "VA-API picture kept a shared output surface".to_string())?;
            assert!(
                self.active_surfaces
                    .insert(
                        surface_id,
                        DecodedSurface::new(surface, decoded_picture),
                    )
                    .is_none(),
                "decoded VA surface was reused while still active"
            );

            let frame = DecodeResult {
                frame: DecodedFrameData { surface_id, resolution: display_resolution },
                metadata: DecodeResultMetadata {
                    pts,
                    pic_order_cnt,
                    max_num_reorder_frames,
                    is_idr,
                    color_space,
                    color_range,
                },
            };
            self.references.push((reference_id, surface_id));
            Ok(Some(frame))
        }

        fn add_buffers(
            &self,
            picture: &mut Picture<PictureNew, Surface<()>>,
            context: &Rc<Context>,
            surface_id: libva::VASurfaceID,
            decode_info: DecodeInformation,
            sps: &SeqParameterSet,
            pps: &PicParameterSet,
        ) -> Result<(), String> {
            let picture_parameter =
                self.picture_parameter(surface_id, &decode_info, sps, pps)?;
            for buffer in [picture_parameter, iq_matrix_parameter(sps, pps)] {
                add_buffer(picture, context, buffer)?;
            }
            for buffer in self.slice_buffers(&decode_info, sps, pps)? {
                add_buffer(picture, context, buffer)?;
            }
            Ok(())
        }

        fn picture_parameter(
            &self,
            surface_id: libva::VASurfaceID,
            decode_info: &DecodeInformation,
            sps: &SeqParameterSet,
            pps: &PicParameterSet,
        ) -> Result<BufferType, String> {
            let seq_fields = H264SeqFields::new(
                chroma_format_idc(sps),
                sps.chroma_info.separate_colour_plane_flag as u32,
                sps.gaps_in_frame_num_value_allowed_flag as u32,
                matches!(&sps.frame_mbs_flags, FrameMbsFlags::Frames) as u32,
                mb_adaptive_frame_field_flag(sps) as u32,
                sps.direct_8x8_inference_flag as u32,
                (sps.level_idc >= 31) as u32,
                sps.log2_max_frame_num_minus4.into(),
                pic_order_cnt_type(sps),
                log2_max_pic_order_cnt_lsb_minus4(sps).into(),
                delta_pic_order_always_zero_flag(sps) as u32,
            );
            let pic_fields = H264PicFields::new(
                pps.entropy_coding_mode_flag as u32,
                pps.weighted_pred_flag as u32,
                pps.weighted_bipred_idc.into(),
                transform_8x8_mode_flag(pps) as u32,
                matches!(&decode_info.header.field_pic, FieldPic::Field(_)) as u32,
                pps.constrained_intra_pred_flag as u32,
                pps.bottom_field_pic_order_in_frame_present_flag as u32,
                pps.deblocking_filter_control_present_flag as u32,
                pps.redundant_pic_cnt_present_flag as u32,
                decode_info.header.dec_ref_pic_marking.is_some() as u32,
            );
            let picture_height_in_mbs_minus1 =
                picture_height_in_mbs_minus1(sps).try_into().map_err(|_| {
                    "H264 picture height does not fit VA-API fields".to_string()
                })?;
            if pps.slice_groups.is_some() {
                return Err(
                    "H264 flexible macroblock ordering is not supported by VA-API".into(),
                );
            }

            let pic_param = PictureParameterBufferH264::new(
                current_picture(surface_id, decode_info),
                self.reference_frames(),
                sps.pic_width_in_mbs_minus1.try_into().map_err(|_| {
                    "H264 picture width does not fit VA-API fields".to_string()
                })?,
                picture_height_in_mbs_minus1,
                sps.chroma_info.bit_depth_luma_minus8,
                sps.chroma_info.bit_depth_chroma_minus8,
                sps.max_num_ref_frames.try_into().unwrap_or(u8::MAX),
                &seq_fields,
                0,
                0,
                0,
                pps.pic_init_qp_minus26 as i8,
                pps.pic_init_qs_minus26 as i8,
                pps.chroma_qp_index_offset as i8,
                second_chroma_qp_index_offset(pps) as i8,
                &pic_fields,
                decode_info.header.frame_num,
            );
            Ok(BufferType::PictureParameter(PictureParameter::H264(pic_param)))
        }

        fn slice_buffers(
            &self,
            decode_info: &DecodeInformation,
            sps: &SeqParameterSet,
            pps: &PicParameterSet,
        ) -> Result<Vec<BufferType>, String> {
            if decode_info.slice_headers.len() != decode_info.slice_data_indices.len()
                || decode_info.slice_headers.len()
                    != decode_info.slice_header_bit_sizes.len()
            {
                return Err("H264 slice metadata is inconsistent".into());
            }

            let mut buffers = Vec::with_capacity(decode_info.slice_headers.len() * 2);
            for (index, header) in decode_info.slice_headers.iter().enumerate() {
                let ref_pic_list_0 =
                    self.reference_list(decode_info.reference_list_l0.as_deref())?;
                let ref_pic_list_1 =
                    self.reference_list(decode_info.reference_list_l1.as_deref())?;
                let offset = decode_info.slice_data_indices[index];
                let next_offset = decode_info
                    .slice_data_indices
                    .get(index + 1)
                    .copied()
                    .unwrap_or(decode_info.slice_data.len());
                let slice_data = decode_info.slice_data[offset..next_offset].to_vec();
                let (weights, denominators) = prediction_weights(header, sps, pps);
                let mut slices = SliceParameterBufferH264::new_array();
                slices.add_slice_parameter(
                    slice_data.len().try_into().unwrap_or(u32::MAX),
                    0,
                    VA_SLICE_DATA_FLAG_ALL,
                    8 + decode_info.slice_header_bit_sizes[index],
                    header.first_mb_in_slice.try_into().unwrap_or(u16::MAX),
                    slice_type(header),
                    header.direct_spatial_mv_pred_flag.unwrap_or(false) as u8,
                    num_ref_idx_l0_active_minus1(header, pps) as u8,
                    num_ref_idx_l1_active_minus1(header, pps) as u8,
                    header.cabac_init_idc.unwrap_or(0).try_into().unwrap_or(0),
                    header.slice_qp_delta.try_into().unwrap_or(0),
                    header.disable_deblocking_filter_idc,
                    0,
                    0,
                    ref_pic_list_0,
                    ref_pic_list_1,
                    denominators.luma,
                    denominators.chroma,
                    weights.luma_l0_flag,
                    weights.luma_l0,
                    weights.luma_offset_l0,
                    weights.chroma_l0_flag,
                    weights.chroma_l0,
                    weights.chroma_offset_l0,
                    weights.luma_l1_flag,
                    weights.luma_l1,
                    weights.luma_offset_l1,
                    weights.chroma_l1_flag,
                    weights.chroma_l1,
                    weights.chroma_offset_l1,
                );
                buffers.push(BufferType::SliceParameter(SliceParameter::H264(slices)));
                buffers.push(BufferType::SliceData(slice_data));
            }
            Ok(buffers)
        }

        fn reference_frames(&self) -> [PictureH264; 16] {
            let mut pictures = invalid_h264_pictures::<16>();
            for (slot, (_, surface_id)) in self.references.iter().take(16).enumerate() {
                let surface = self
                    .active_surfaces
                    .get(surface_id)
                    .expect("H264 reference points to a missing VA surface");
                let picture = surface
                    .reference
                    .expect("H264 reference points to a non-reference VA surface");
                pictures[slot] = picture.to_va_picture(*surface_id);
            }
            pictures
        }

        fn reference_list(
            &self,
            references: Option<&[ReferencePictureInfo]>,
        ) -> Result<[PictureH264; 32], String> {
            let mut pictures = invalid_h264_pictures::<32>();
            for (slot, reference) in references.unwrap_or(&[]).iter().take(32).enumerate()
            {
                let surface_id = self
                    .references
                    .iter()
                    .find(|(id, _)| *id == reference.id)
                    .map(|(_, surface_id)| *surface_id)
                    .ok_or_else(|| {
                        format!("missing VA-API H264 reference {:?}", reference.id)
                    })?;
                pictures[slot] = reference_picture(reference, surface_id);
            }
            Ok(pictures)
        }

        fn take_surface(
            &mut self,
            resolution: VideoResolution,
        ) -> Result<Surface<()>, String> {
            take_nv12_surface(
                &self.display,
                &mut self.free_surfaces,
                resolution,
                UsageHint::USAGE_HINT_DECODER | UsageHint::USAGE_HINT_EXPORT,
                DECODE_SURFACE_ALLOCATION_BATCH,
                "decode",
            )
        }

        fn sort_frame(
            &mut self,
            frame: DecodeResult<DecodedFrameData>,
        ) -> Vec<OutputFrame<DecodedFrameData>> {
            self.frame_sorter.put(frame)
        }

        fn flush_sorted_frames(&mut self) -> Vec<OutputFrame<DecodedFrameData>> {
            self.frame_sorter.flush()
        }

        fn discard_sorted_frames(&mut self) {
            for frame in self.frame_sorter.flush() {
                self.release_output_surface(frame.data.surface_id);
            }
        }

        fn drop_reference(&mut self, reference_id: ReferenceId) -> Result<(), String> {
            let index = self
                .references
                .iter()
                .position(|(id, _)| *id == reference_id)
                .ok_or_else(|| format!("missing VA-API H264 reference {reference_id:?}"))?;
            let (_, surface_id) = self.references.remove(index);
            self.active_surface_mut(surface_id)?.clear_reference();
            self.recycle_surface(surface_id);
            Ok(())
        }

        fn retain_references(&mut self) -> Result<(), String> {
            let references = std::mem::take(&mut self.references);
            for (_, surface_id) in references {
                self.active_surface_mut(surface_id)?.clear_reference();
                self.recycle_surface(surface_id);
            }
            Ok(())
        }

        fn retire_session_surfaces(&mut self, reusable: bool) {
            for surface in self.active_surfaces.values_mut() {
                surface.retire_session(reusable);
            }
            let surface_ids = self.active_surfaces.keys().copied().collect::<Vec<_>>();
            self.references.clear();
            if !reusable {
                self.free_surfaces.clear();
            }
            for surface_id in surface_ids {
                self.recycle_surface(surface_id);
            }
        }

        fn surface(
            &self,
            surface_id: libva::VASurfaceID,
        ) -> Result<&Surface<()>, String> {
            self.active_surfaces
                .get(&surface_id)
                .map(|surface| &surface.surface)
                .ok_or_else(|| format!("missing decoded VA surface {surface_id}"))
        }

        fn active_surface_mut(
            &mut self,
            surface_id: libva::VASurfaceID,
        ) -> Result<&mut DecodedSurface, String> {
            self.active_surfaces
                .get_mut(&surface_id)
                .ok_or_else(|| format!("missing decoded VA surface {surface_id}"))
        }

        fn release_output_surface(&mut self, surface_id: libva::VASurfaceID) {
            if let Some(surface) = self.active_surfaces.get_mut(&surface_id) {
                surface.release_output();
            }
            self.recycle_surface(surface_id);
        }

        fn recycle_surface(&mut self, surface_id: libva::VASurfaceID) {
            let Some(surface) = self.active_surfaces.get(&surface_id) else {
                return;
            };
            if !surface.is_recyclable() {
                return;
            }
            let surface = self.active_surfaces.remove(&surface_id).unwrap();
            if surface.reusable {
                self.free_surfaces.push(surface.surface);
            }
        }
    }

    impl WgpuTexturesDecoder {
        pub fn new(
            device: Arc<wgpu::Device>,
            queue: Arc<wgpu::Queue>,
            adapter_info: Option<&wgpu::AdapterInfo>,
        ) -> Result<Self, VaapiH264DecoderError> {
            Ok(Self { decoder: H264Decoder::new(adapter_info)?, device, queue })
        }

        /// The produced textures have the [`wgpu::TextureFormat::NV12`] format and can be used as texture bindings.
        pub fn decode(
            &mut self,
            frame: EncodedInputChunk<'_>,
        ) -> Result<Vec<OutputFrame<wgpu::Texture>>, VaapiH264DecoderError> {
            self.process_event(H264DecoderEvent::DecodeChunk(frame))
        }

        /// Flush all frames from the decoder.
        ///
        /// Make sure this is done only when no more frames are coming that need
        /// to be presented before the already decoded frames.
        pub fn flush(
            &mut self,
        ) -> Result<Vec<OutputFrame<wgpu::Texture>>, VaapiH264DecoderError> {
            self.process_event(H264DecoderEvent::Flush)
        }

        /// Process a [`H264DecoderEvent`]. For most use cases, [`Self::decode`]
        /// and [`Self::flush`] are enough.
        ///
        /// Use this when you need fine-grained control over parser frame
        /// boundaries, parsed access units, or data-loss signaling.
        pub fn process_event(
            &mut self,
            event: H264DecoderEvent<'_>,
        ) -> Result<Vec<OutputFrame<wgpu::Texture>>, VaapiH264DecoderError> {
            let frames = self.decoder.process_event(event)?;
            self.copy_frames(frames)
        }

        fn copy_frames(
            &mut self,
            frames: Vec<OutputFrame<DecodedFrameData>>,
        ) -> Result<Vec<OutputFrame<wgpu::Texture>>, VaapiH264DecoderError> {
            frames.into_iter().map(|frame| self.copy_frame(frame)).collect()
        }

        fn copy_frame(
            &mut self,
            frame: OutputFrame<DecodedFrameData>,
        ) -> Result<OutputFrame<wgpu::Texture>, VaapiH264DecoderError> {
            let surface_id = frame.data.surface_id;
            let result = (|| {
                let dmabuf = export_surface_as_frame(
                    &self.device,
                    self.decoder
                        .surface(surface_id)
                        .map_err(VaapiH264DecoderError::Decode)?,
                )
                .map_err(VaapiH264DecoderError::Decode)?;
                let size = wgpu::Extent3d {
                    width: frame.data.resolution.width,
                    height: frame.data.resolution.height,
                    depth_or_array_layers: 1,
                };
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("VA-API H264 decoder output texture"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::NV12,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let mut encoder =
                    self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("VA-API H264 decoder output copy"),
                    });
                encoder.copy_texture_to_texture(
                    dmabuf.texture().as_image_copy(),
                    texture.as_image_copy(),
                    size,
                );
                self.queue.submit([encoder.finish()]);
                self.device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .map_err(|err| VaapiH264DecoderError::Decode(err.to_string()))?;
                Ok(OutputFrame { data: texture, metadata: frame.metadata })
            })();
            self.decoder.release_output_surface(surface_id);
            result
        }
    }

    impl Drop for H264Decoder {
        fn drop(&mut self) {
            self.discard_sorted_frames();
            self.retire_session_surfaces(false);
        }
    }

    struct VaapiDecodeSession {
        _config: Config,
        context: Rc<Context>,
        stream: VaapiStreamInfo,
    }

    impl VaapiDecodeSession {
        fn new(display: &Rc<Display>, stream: VaapiStreamInfo) -> Result<Self, String> {
            let entrypoints =
                display.query_config_entrypoints(stream.profile).map_err(|err| {
                    format!("failed to query VA-API H264 entrypoints: {err}")
                })?;
            if !entrypoints.contains(&VAEntrypoint::VAEntrypointVLD) {
                return Err("VA-API H264 VLD entrypoint is unavailable".into());
            }
            let config = display
                .create_config(
                    vec![VAConfigAttrib {
                        type_: VAConfigAttribType::VAConfigAttribRTFormat,
                        value: stream.rt_format,
                    }],
                    stream.profile,
                    VAEntrypoint::VAEntrypointVLD,
                )
                .map_err(|err| format!("failed to create VA-API H264 config: {err}"))?;
            let context = display
                .create_context::<()>(
                    &config,
                    stream.coded_resolution.width,
                    stream.coded_resolution.height,
                    None,
                    true,
                )
                .map_err(|err| format!("failed to create VA-API H264 context: {err}"))?;

            Ok(Self { _config: config, context, stream })
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct VaapiStreamInfo {
        profile: VAProfile::Type,
        rt_format: u32,
        coded_resolution: VideoResolution,
        display_resolution: VideoResolution,
        max_num_reorder_frames: u64,
    }

    impl VaapiStreamInfo {
        fn from_sps(sps: &SeqParameterSet) -> Result<Self, String> {
            if !matches!(&sps.frame_mbs_flags, FrameMbsFlags::Frames) {
                return Err(
                    "interlaced H264 streams are not supported by this VA-API decoder"
                        .into(),
                );
            }
            let profile = va_profile(sps)?;
            let rt_format = va_rt_format(sps)?;
            let coded_resolution = VideoResolution {
                width: (sps.pic_width_in_mbs_minus1 + 1) * 16,
                height: (sps.pic_height_in_map_units_minus1 + 1) * 16,
            };
            let (width, height) = sps
                .pixel_dimensions()
                .map_err(|err| format!("invalid H264 display dimensions: {err:?}"))?;
            Ok(Self {
                profile,
                rt_format,
                coded_resolution,
                display_resolution: VideoResolution { width, height },
                max_num_reorder_frames: h264_max_num_reorder_frames(sps)?,
            })
        }
    }

    #[derive(Clone, Copy)]
    struct DecodedPictureInfo {
        frame_num: u16,
        pic_order_cnt: [i32; 2],
        long_term_pic_num: Option<u64>,
    }

    impl DecodedPictureInfo {
        fn from_decode_info(decode_info: &DecodeInformation) -> Self {
            Self {
                frame_num: decode_info.picture_info.FrameNum,
                pic_order_cnt: decode_info.picture_info.PicOrderCnt_as_reference_pic,
                long_term_pic_num: current_long_term_pic_num(decode_info),
            }
        }

        fn to_va_picture(self, surface_id: libva::VASurfaceID) -> PictureH264 {
            va_picture(
                surface_id,
                self.frame_num.into(),
                self.pic_order_cnt,
                self.long_term_pic_num,
                true,
            )
        }
    }

    fn current_picture(
        surface_id: libva::VASurfaceID,
        decode_info: &DecodeInformation,
    ) -> PictureH264 {
        va_picture(
            surface_id,
            decode_info.picture_info.FrameNum.into(),
            decode_info.picture_info.PicOrderCnt_for_decoding,
            current_long_term_pic_num(decode_info),
            decode_info.header.dec_ref_pic_marking.is_some(),
        )
    }

    fn reference_picture(
        reference: &ReferencePictureInfo,
        surface_id: libva::VASurfaceID,
    ) -> PictureH264 {
        va_picture(
            surface_id,
            reference.FrameNum.into(),
            reference.PicOrderCnt,
            reference.LongTermPicNum,
            true,
        )
    }

    fn va_picture(
        surface_id: libva::VASurfaceID,
        frame_num: u64,
        pic_order_cnt: [i32; 2],
        long_term_pic_num: Option<u64>,
        reference: bool,
    ) -> PictureH264 {
        let flags = match (long_term_pic_num, reference) {
            (Some(_), _) => VA_PICTURE_H264_LONG_TERM_REFERENCE,
            (None, true) => VA_PICTURE_H264_SHORT_TERM_REFERENCE,
            (None, false) => 0,
        };
        PictureH264::new(
            surface_id,
            long_term_pic_num.unwrap_or(frame_num) as u32,
            flags,
            pic_order_cnt[0],
            pic_order_cnt[1],
        )
    }

    fn current_long_term_pic_num(decode_info: &DecodeInformation) -> Option<u64> {
        match decode_info.header.dec_ref_pic_marking.as_ref()? {
            DecRefPicMarking::Idr { long_term_reference_flag, .. } => {
                long_term_reference_flag.then_some(0)
            }
            DecRefPicMarking::Adaptive(operations) => operations.iter().find_map(|op| {
                if let MemoryManagementControlOperation::CurrentUsedForLongTerm {
                    long_term_frame_idx,
                } = op
                {
                    Some((*long_term_frame_idx).into())
                } else {
                    None
                }
            }),
            DecRefPicMarking::SlidingWindow => None,
        }
    }

    fn add_buffer(
        picture: &mut Picture<PictureNew, Surface<()>>,
        context: &Rc<Context>,
        buffer: BufferType,
    ) -> Result<(), String> {
        let buffer = context
            .create_buffer(buffer)
            .map_err(|err| format!("failed to create VA-API buffer: {err}"))?;
        picture.add_buffer(buffer);
        Ok(())
    }

    fn va_profile(sps: &SeqParameterSet) -> Result<VAProfile::Type, String> {
        match sps.profile() {
            Profile::Baseline if sps.constraint_flags.flag0() => {
                Ok(VAProfile::VAProfileH264ConstrainedBaseline)
            }
            Profile::Baseline => {
                Err("unsupported unconstrained H264 Baseline profile".into())
            }
            Profile::Main => Ok(VAProfile::VAProfileH264Main),
            Profile::Extended if sps.constraint_flags.flag1() => {
                Ok(VAProfile::VAProfileH264Main)
            }
            Profile::Extended => {
                Err("unsupported unconstrained H264 Extended profile".into())
            }
            Profile::High | Profile::High10 | Profile::High422 => {
                Ok(VAProfile::VAProfileH264High)
            }
            profile => Err(format!("unsupported H264 profile {profile:?}")),
        }
    }

    fn va_rt_format(sps: &SeqParameterSet) -> Result<u32, String> {
        match (sps.chroma_info.bit_depth_luma_minus8 + 8, sps.chroma_info.chroma_format) {
            (8, ChromaFormat::Monochrome | ChromaFormat::YUV420) => {
                Ok(VA_RT_FORMAT_YUV420)
            }
            (depth, format) => Err(format!(
                "unsupported H264 VA-API surface format: {depth}-bit {format:?}"
            )),
        }
    }

    fn chroma_format_idc(sps: &SeqParameterSet) -> u32 {
        match sps.chroma_info.chroma_format {
            ChromaFormat::Monochrome => 0,
            ChromaFormat::YUV420 => 1,
            ChromaFormat::YUV422 => 2,
            ChromaFormat::YUV444 => 3,
            ChromaFormat::Invalid(value) => value,
        }
    }

    fn mb_adaptive_frame_field_flag(sps: &SeqParameterSet) -> bool {
        match &sps.frame_mbs_flags {
            FrameMbsFlags::Frames => false,
            FrameMbsFlags::Fields { mb_adaptive_frame_field_flag } => {
                *mb_adaptive_frame_field_flag
            }
        }
    }

    fn pic_order_cnt_type(sps: &SeqParameterSet) -> u32 {
        match &sps.pic_order_cnt {
            PicOrderCntType::TypeZero { .. } => 0,
            PicOrderCntType::TypeOne { .. } => 1,
            PicOrderCntType::TypeTwo => 2,
        }
    }

    fn log2_max_pic_order_cnt_lsb_minus4(sps: &SeqParameterSet) -> u8 {
        match &sps.pic_order_cnt {
            PicOrderCntType::TypeZero { log2_max_pic_order_cnt_lsb_minus4 } => {
                *log2_max_pic_order_cnt_lsb_minus4
            }
            _ => 0,
        }
    }

    fn delta_pic_order_always_zero_flag(sps: &SeqParameterSet) -> bool {
        match &sps.pic_order_cnt {
            PicOrderCntType::TypeOne { delta_pic_order_always_zero_flag, .. } => {
                *delta_pic_order_always_zero_flag
            }
            _ => false,
        }
    }

    fn picture_height_in_mbs_minus1(sps: &SeqParameterSet) -> u32 {
        let interlaced = (!matches!(&sps.frame_mbs_flags, FrameMbsFlags::Frames)) as u32;
        ((sps.pic_height_in_map_units_minus1 + 1) << interlaced) - 1
    }

    fn transform_8x8_mode_flag(pps: &PicParameterSet) -> bool {
        pps.extension.as_ref().is_some_and(|extra| extra.transform_8x8_mode_flag)
    }

    fn second_chroma_qp_index_offset(pps: &PicParameterSet) -> i32 {
        pps.extension
            .as_ref()
            .map(|extra| extra.second_chroma_qp_index_offset)
            .unwrap_or(pps.chroma_qp_index_offset)
    }

    fn iq_matrix_parameter(sps: &SeqParameterSet, pps: &PicParameterSet) -> BufferType {
        let mut scaling_list4x4 = [[16; 16]; 6];
        let mut scaling_list8x8 = [[16; 64]; 2];

        if let Some(matrix) = sps.chroma_info.scaling_matrix.as_ref() {
            fill_scaling_4x4(&matrix.scaling_list4x4, &mut scaling_list4x4);
            fill_scaling_8x8(&matrix.scaling_list8x8, &mut scaling_list8x8);
        }
        if let Some(matrix) =
            pps.extension.as_ref().and_then(|extra| extra.pic_scaling_matrix.as_ref())
        {
            fill_scaling_4x4(&matrix.scaling_list4x4, &mut scaling_list4x4);
            if let Some(scaling) = matrix.scaling_list8x8.as_ref() {
                fill_scaling_8x8(scaling, &mut scaling_list8x8);
            }
        }

        BufferType::IQMatrix(IQMatrix::H264(IQMatrixBufferH264::new(
            scaling_list4x4,
            scaling_list8x8,
        )))
    }

    fn fill_scaling_4x4(source: &[ScalingList<16>], target: &mut [[u8; 16]; 6]) {
        for (source, target) in source.iter().zip(target.iter_mut()) {
            if let ScalingList::List(values) = source {
                let zigzag = values.map(|value| value.get());
                get_raster_from_zigzag_4x4(zigzag, target);
            }
        }
    }

    fn fill_scaling_8x8(source: &[ScalingList<64>], target: &mut [[u8; 64]; 2]) {
        for (source, target) in source.iter().zip(target.iter_mut()) {
            if let ScalingList::List(values) = source {
                let zigzag = values.map(|value| value.get());
                get_raster_from_zigzag_8x8(zigzag, target);
            }
        }
    }

    fn get_raster_from_zigzag_4x4(src: [u8; 16], dst: &mut [u8; 16]) {
        const ZIGZAG: [usize; 16] =
            [0, 1, 4, 8, 5, 2, 3, 6, 9, 12, 13, 10, 7, 11, 14, 15];
        for i in 0..16 {
            dst[ZIGZAG[i]] = src[i];
        }
    }

    fn get_raster_from_zigzag_8x8(src: [u8; 64], dst: &mut [u8; 64]) {
        const ZIGZAG: [usize; 64] = [
            0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40,
            48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29,
            22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54,
            47, 55, 62, 63,
        ];
        for i in 0..64 {
            dst[ZIGZAG[i]] = src[i];
        }
    }

    #[derive(Clone, Copy, Default)]
    struct PredictionDenominators {
        luma: u8,
        chroma: u8,
    }

    #[derive(Clone, Copy, Default)]
    struct PredictionWeights {
        luma_l0_flag: u8,
        luma_l0: [i16; 32],
        luma_offset_l0: [i16; 32],
        chroma_l0_flag: u8,
        chroma_l0: [[i16; 2]; 32],
        chroma_offset_l0: [[i16; 2]; 32],
        luma_l1_flag: u8,
        luma_l1: [i16; 32],
        luma_offset_l1: [i16; 32],
        chroma_l1_flag: u8,
        chroma_l1: [[i16; 2]; 32],
        chroma_offset_l1: [[i16; 2]; 32],
    }

    fn prediction_weights(
        header: &SliceHeader,
        sps: &SeqParameterSet,
        pps: &PicParameterSet,
    ) -> (PredictionWeights, PredictionDenominators) {
        let mut weights = PredictionWeights::default();
        let Some(table) = header.pred_weight_table.as_ref() else {
            return (weights, PredictionDenominators::default());
        };

        fill_l0_prediction_weights(&mut weights, table);
        if sps.chroma_info.chroma_format != ChromaFormat::Monochrome {
            fill_l0_chroma_prediction_weights(&mut weights, table);
        }

        if pps.weighted_pred_flag && matches!(&header.slice_type.family, SliceFamily::P) {
            weights.luma_l0_flag = 1;
            weights.chroma_l0_flag =
                (sps.chroma_info.chroma_format != ChromaFormat::Monochrome) as u8;
        }

        (
            weights,
            PredictionDenominators {
                luma: table.luma_log2_weight_denom.try_into().unwrap_or(u8::MAX),
                chroma: table
                    .chroma_log2_weight_denom
                    .unwrap_or_default()
                    .try_into()
                    .unwrap_or(u8::MAX),
            },
        )
    }

    fn fill_l0_prediction_weights(
        weights: &mut PredictionWeights,
        table: &PredWeightTable,
    ) {
        let default_weight =
            1i16.checked_shl(table.luma_log2_weight_denom).unwrap_or_default();
        for (index, weight) in table.luma_weights.iter().take(32).enumerate() {
            match weight {
                Some(weight) => {
                    weights.luma_l0[index] = weight.weight.try_into().unwrap_or(0);
                    weights.luma_offset_l0[index] = weight.offset.try_into().unwrap_or(0);
                }
                None => {
                    weights.luma_l0[index] = default_weight;
                }
            }
        }
    }

    fn fill_l0_chroma_prediction_weights(
        weights: &mut PredictionWeights,
        table: &PredWeightTable,
    ) {
        let default_weight = 1i16
            .checked_shl(table.chroma_log2_weight_denom.unwrap_or_default())
            .unwrap_or_default();
        for (index, chroma_weights) in table.chroma_weights.iter().take(32).enumerate() {
            for component in 0..2 {
                if let Some(weight) = chroma_weights.get(component) {
                    weights.chroma_l0[index][component] =
                        weight.weight.try_into().unwrap_or(0);
                    weights.chroma_offset_l0[index][component] =
                        weight.offset.try_into().unwrap_or(0);
                } else {
                    weights.chroma_l0[index][component] = default_weight;
                }
            }
        }
    }

    fn num_ref_idx_l0_active_minus1(header: &SliceHeader, pps: &PicParameterSet) -> u32 {
        header
            .num_ref_idx_active
            .as_ref()
            .map(|num| match num {
                NumRefIdxActive::P { num_ref_idx_l0_active_minus1 }
                | NumRefIdxActive::B { num_ref_idx_l0_active_minus1, .. } => {
                    *num_ref_idx_l0_active_minus1
                }
            })
            .unwrap_or(pps.num_ref_idx_l0_default_active_minus1)
    }

    fn num_ref_idx_l1_active_minus1(header: &SliceHeader, pps: &PicParameterSet) -> u32 {
        header
            .num_ref_idx_active
            .as_ref()
            .and_then(|num| match num {
                NumRefIdxActive::B { num_ref_idx_l1_active_minus1, .. } => {
                    Some(*num_ref_idx_l1_active_minus1)
                }
                NumRefIdxActive::P { .. } => None,
            })
            .unwrap_or(pps.num_ref_idx_l1_default_active_minus1)
    }

    fn slice_type(header: &SliceHeader) -> u8 {
        match &header.slice_type.family {
            SliceFamily::P => 0,
            SliceFamily::B => 1,
            SliceFamily::I => 2,
            SliceFamily::SP => 3,
            SliceFamily::SI => 4,
        }
    }

    #[cfg(all(test, target_os = "linux"))]
    mod tests {
        use std::{
            fs,
            path::{Path, PathBuf},
            process::Command,
            sync::Mutex,
            sync::mpsc,
            time::{SystemTime, UNIX_EPOCH},
        };

        use super::*;

        const TEST_WIDTH: u32 = 64;
        const TEST_HEIGHT: u32 = 64;
        const TEST_FRAME_COUNT: usize = 4;
        static VAAPI_TEST_LOCK: Mutex<()> = Mutex::new(());

        #[test]
        #[ignore = "requires ffmpeg and a VA-API capable Linux host"]
        fn decodes_ffmpeg_annexb_stream_to_nv12_dmabuf_frames() {
            let _guard = VAAPI_TEST_LOCK.lock().unwrap();
            let video = GeneratedVideo::new("stream.h264", "h264", 0);
            assert_decodes_like_ffmpeg(&video);
        }

        #[test]
        #[ignore = "requires ffmpeg and a VA-API capable Linux host"]
        fn decodes_ffmpeg_annexb_b_frames_in_display_order() {
            let _guard = VAAPI_TEST_LOCK.lock().unwrap();
            let video = GeneratedVideo::new("bframes.h264", "h264", 2);
            assert_decodes_like_ffmpeg(&video);
        }

        fn assert_decodes_like_ffmpeg(video: &GeneratedVideo) {
            let stream = fs::read(&video.path).expect("failed to read generated stream");
            let (device, queue, adapter_info) = crate::test_wgpu_device_and_queue();
            let queue = Arc::new(queue);
            let mut decoder = WgpuTexturesDecoder::new(
                Arc::clone(&device),
                Arc::clone(&queue),
                Some(&adapter_info),
            )
            .expect("failed to create decoder");

            let mut frames = decoder
                .process_event(H264DecoderEvent::DecodeChunk(EncodedInputChunk {
                    data: &stream,
                    pts: Some(0),
                }))
                .expect("failed to decode stream");
            frames.extend(
                decoder
                    .process_event(H264DecoderEvent::SignalFrameEnd)
                    .expect("failed to flush frame"),
            );
            frames.extend(decoder.flush().expect("failed to flush decoder"));

            assert_eq!(frames.len(), TEST_FRAME_COUNT);
            let expected_frames = ffmpeg_nv12_frames(&video.path);
            for (index, (actual, expected)) in
                frames.into_iter().zip(expected_frames).enumerate()
            {
                let actual = readback_nv12_frame(&device, &queue, actual);
                assert_eq!(
                    actual, expected,
                    "VA-API decoded frame {index} differs from ffmpeg NV12 decode"
                );
            }
        }

        fn readback_nv12_frame(
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            frame: OutputFrame<wgpu::Texture>,
        ) -> Vec<u8> {
            let texture = &frame.data;
            let mut output = download_texture_plane(
                device,
                queue,
                texture,
                wgpu::TextureAspect::Plane0,
                TEST_WIDTH,
                TEST_HEIGHT,
                1,
            );
            output.extend(download_texture_plane(
                device,
                queue,
                texture,
                wgpu::TextureAspect::Plane1,
                TEST_WIDTH / 2,
                TEST_HEIGHT / 2,
                2,
            ));
            output
        }

        fn download_texture_plane(
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            texture: &wgpu::Texture,
            aspect: wgpu::TextureAspect,
            width: u32,
            height: u32,
            bytes_per_pixel: u32,
        ) -> Vec<u8> {
            let row_bytes = width * bytes_per_pixel;
            let padded_row_bytes = pad_to_256(row_bytes);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("NV12 readback buffer"),
                size: (padded_row_bytes * height) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("NV12 readback encoder"),
                });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row_bytes),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
            queue.submit(Some(encoder.finish()));

            let slice = buffer.slice(..);
            let (sender, receiver) = mpsc::sync_channel(1);
            slice.map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).ok();
            });
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            receiver
                .recv()
                .expect("failed to receive NV12 readback result")
                .expect("failed to map NV12 readback buffer");

            let mapped =
                slice.get_mapped_range().expect("failed to read mapped NV12 buffer");
            let mut output = Vec::with_capacity((row_bytes * height) as usize);
            for row in mapped.chunks(padded_row_bytes as usize).take(height as usize) {
                output.extend_from_slice(&row[..row_bytes as usize]);
            }
            drop(mapped);
            buffer.unmap();
            output
        }

        fn pad_to_256(value: u32) -> u32 {
            value.div_ceil(256) * 256
        }

        struct GeneratedVideo {
            path: PathBuf,
            dir: PathBuf,
        }

        impl GeneratedVideo {
            fn new(filename: &str, muxer: &str, b_frames: usize) -> Self {
                let dir = std::env::temp_dir().join(format!(
                    "smelter-vaapi-h264-{}",
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
                ));
                fs::create_dir(&dir).expect("failed to create temp dir");
                let path = dir.join(filename);
                generate_video(&path, muxer, b_frames);
                Self { path, dir }
            }
        }

        impl Drop for GeneratedVideo {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.dir).ok();
            }
        }

        fn generate_video(output: &Path, muxer: &str, b_frames: usize) {
            let input = format!("testsrc2=size={TEST_WIDTH}x{TEST_HEIGHT}:rate=5");
            let frame_count = TEST_FRAME_COUNT.to_string();
            let mut command = Command::new("ffmpeg");
            command.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                &input,
                "-frames:v",
                &frame_count,
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-preset",
                "veryfast",
                "-g",
                &frame_count,
            ]);
            if b_frames == 0 {
                command.args(["-tune", "zerolatency", "-bf", "0"]);
            } else {
                command.args([
                    "-bf",
                    &b_frames.to_string(),
                    "-x264-params",
                    "b-adapt=0:scenecut=0",
                ]);
            }
            let status = command
                .args(["-f", muxer])
                .arg(output)
                .status()
                .expect("failed to execute ffmpeg");
            assert!(status.success(), "ffmpeg failed with status {status}");
        }

        fn ffmpeg_nv12_frames(input: &Path) -> Vec<Vec<u8>> {
            let frame_count = TEST_FRAME_COUNT.to_string();
            let output = Command::new("ffmpeg")
                .args(["-hide_banner", "-loglevel", "error", "-i"])
                .arg(input)
                .args([
                    "-frames:v",
                    &frame_count,
                    "-pix_fmt",
                    "nv12",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .output()
                .expect("failed to execute ffmpeg");
            assert!(
                output.status.success(),
                "ffmpeg failed with status {}",
                output.status
            );

            let frame_size = (TEST_WIDTH * TEST_HEIGHT * 3 / 2) as usize;
            assert_eq!(output.stdout.len(), TEST_FRAME_COUNT * frame_size);
            output.stdout.chunks(frame_size).map(|chunk| chunk.to_vec()).collect()
        }
    }
}

pub use imp::{VaapiH264DecoderError, WgpuTexturesDecoder};
