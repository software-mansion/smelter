#include "callback.h"
#include "api.h"
#include "enums.h"

HRESULT InputCallbackWrapper::VideoInputFrameArrived(
    IDeckLinkVideoInputFrame *video_frame,
    IDeckLinkAudioInputPacket *audio_packet) {
  auto result = this->cb->video_input_frame_arrived(video_frame, audio_packet);
  return static_cast<HRESULT>(result);
}

HRESULT InputCallbackWrapper::VideoInputFormatChanged(
    BMDVideoInputFormatChangedEvents events, IDeckLinkDisplayMode *display_mode,
    BMDDetectedVideoInputFormatFlags flags) {
  auto result = this->cb->video_input_format_changed(
      into_video_input_format_changed_events(events), display_mode,
      into_detected_video_input_format_flags(flags));
  return static_cast<HRESULT>(result);
}
