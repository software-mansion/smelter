use h264_reader::nal::{pps::PicParameterSet, sps::SeqParameterSet};

use crate::parser::{
    h264::{AccessUnit, ParsedNalu},
    reference_manager::DecodeInformation,
    reference_manager::{ReferenceContext, ReferenceId, ReferenceManagementError},
};

#[derive(Debug, Clone)]
pub(crate) enum DecoderInstruction {
    Decode {
        decode_info: DecodeInformation,
        #[cfg_attr(video_toolbox, allow(unused))]
        reference_id: ReferenceId,
    },

    Idr {
        decode_info: DecodeInformation,
        #[cfg_attr(video_toolbox, allow(unused))]
        reference_id: ReferenceId,
    },

    Drop {
        #[cfg_attr(video_toolbox, allow(unused))]
        reference_ids: Vec<ReferenceId>,
    },

    Sps {
        sps: SeqParameterSet,

        #[cfg_attr(vulkan, expect(unused))]
        raw_bytes: Box<[u8]>,
    },

    Pps {
        pps: PicParameterSet,

        #[cfg_attr(vulkan, expect(unused))]
        raw_bytes: Box<[u8]>,
    },
}

pub(crate) fn compile_to_decoder_instructions(
    reference_ctx: &mut ReferenceContext,
    access_units: Vec<AccessUnit>,
) -> Result<Vec<DecoderInstruction>, ReferenceManagementError> {
    let mut instructions = Vec::new();
    for AccessUnit(nalus) in access_units {
        let mut slices = Vec::new();
        for nalu in nalus {
            match nalu.parsed {
                ParsedNalu::Sps(sps) => instructions.push(DecoderInstruction::Sps {
                    sps,
                    raw_bytes: nalu.raw_bytes,
                }),
                ParsedNalu::Pps(pps) => instructions.push(DecoderInstruction::Pps {
                    pps,
                    raw_bytes: nalu.raw_bytes,
                }),
                ParsedNalu::Slice(slice) => {
                    slices.push((slice, nalu.pts));
                }

                ParsedNalu::Other(_) => {}
            }
        }

        // TODO: warn when not all pts are equal here
        let mut inst = reference_ctx.put_picture(slices)?;
        instructions.append(&mut inst);
    }

    Ok(instructions)
}
