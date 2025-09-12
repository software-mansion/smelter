import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';

export function intoOutputEosCondition(
  condition: Outputs.OutputEndCondition
): Api.OutputEndCondition {
  if ('anyOf' in condition) {
    return { any_of: condition.anyOf };
  } else if ('allOf' in condition) {
    return { all_of: condition.allOf };
  } else if ('allInputs' in condition) {
    return { all_inputs: condition.allInputs };
  } else if ('anyInput' in condition) {
    return { any_input: condition.anyInput };
  } else {
    throw new Error('Invalid "send_eos_when" value.');
  }
}

export function intoVulkanH264EncoderBitrate(
  bitrate: Outputs.VulkanH264EncoderBitrate
): Api.VulkanH264EncoderBitrate {
  if (typeof bitrate === 'number') {
    return bitrate;
  }

  return {
    average_bitrate: bitrate.averageBitrate,
    max_bitrate: bitrate.maxBitrate,
  };
}
