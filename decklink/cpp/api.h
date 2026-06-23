#pragma once

#include "decklink/decklink_sdk/include/DeckLinkAPI.h"
#include "enums.h"

#include "decklink/src/api.rs.h"
#include <cstdint>

rust::Vec<IDeckLinkPtr> get_decklinks();

HResult decklink_profile_attributes(IDeckLink *, IDeckLinkProfileAttributes *&);
HResult decklink_input(IDeckLink *, IDeckLinkInput *&);
HResult decklink_profile_manager(IDeckLink *, IDeckLinkProfileManager *&);
HResult decklink_configuration(IDeckLink *, IDeckLinkConfiguration *&);
void decklink_release(IDeckLink *decklink);

// IDeckLinkProfileAttributes
HResult profile_attributes_flag(IDeckLinkProfileAttributes *attrs,
                                FlagAttributeId id, bool &out);
HResult profile_attributes_integer(IDeckLinkProfileAttributes *attrs,
                                   IntegerAttributeId id, int64_t &out);
HResult profile_attributes_float(IDeckLinkProfileAttributes *attrs,
                                 FloatAttributeId id, double &out);
HResult profile_attributes_string(IDeckLinkProfileAttributes *attrs,
                                  StringAttributeId id, rust::String &out,
                                  bool is_static);
void profile_attributes_release(IDeckLinkProfileAttributes *attrs);

// IDeckLinkInput
HResult input_supports_video_mode(IDeckLinkInput *, VideoConnection,
                                  DisplayModeType, PixelFormat,
                                  VideoInputConversionMode,
                                  SupportedVideoModeFlags, DisplayModeType &,
                                  bool &);
HResult input_enable_video(IDeckLinkInput *input, DisplayModeType mode,
                           PixelFormat format, VideoInputFlags flags);
HResult input_enable_audio(IDeckLinkInput *input, uint32_t sample_rate,
                           AudioSampleType sample_type, uint32_t channels);
HResult input_set_callback(IDeckLinkInput *input,
                           rust::Box<DynInputCallback> cb);
HResult input_start_streams(IDeckLinkInput *input);
HResult input_stop_streams(IDeckLinkInput *input);
HResult input_pause_streams(IDeckLinkInput *input);
HResult input_flush_streams(IDeckLinkInput *input);
void input_release(IDeckLinkInput *input);

// IDeckLinkProfileManager
HResult profile_manager_profiles(IDeckLinkProfileManager *,
                                 rust::Vec<IDeckLinkProfilePtr> &);
void profile_manager_release(IDeckLinkProfileManager *);

// IDeckLinkProfile
HResult profile_profile_attributes(IDeckLinkProfile *,
                                   IDeckLinkProfileAttributes *&);
HResult profile_is_active(IDeckLinkProfile *, bool &);
void profile_release(IDeckLinkProfile *);

// IDeckLinkConfiguration
HResult configuration_flag(IDeckLinkConfiguration *conf, FlagConfigurationId id,
                           bool &out);
HResult configuration_integer(IDeckLinkConfiguration *conf,
                              IntegerConfigurationId id, int64_t &out);
HResult configuration_float(IDeckLinkConfiguration *conf,
                            FloatConfigurationId id, double &out);
HResult configuration_string(IDeckLinkConfiguration *conf,
                             StringConfigurationId id, rust::String &out);
HResult configuration_set_flag(IDeckLinkConfiguration *conf,
                               FlagConfigurationId id, bool value);
HResult configuration_set_integer(IDeckLinkConfiguration *conf,
                                  IntegerConfigurationId id, int64_t value);
HResult configuration_set_float(IDeckLinkConfiguration *conf,
                                FloatConfigurationId id, double value);
HResult configuration_set_string(IDeckLinkConfiguration *conf,
                                 StringConfigurationId id, rust::String value);
void configuration_release(IDeckLinkConfiguration *conf);

// IDeckLinkVideoInputFrame
long video_input_frame_width(IDeckLinkVideoInputFrame *frame);
long video_input_frame_height(IDeckLinkVideoInputFrame *frame);
long video_input_frame_row_bytes(IDeckLinkVideoInputFrame *frame);
uint8_t *video_input_frame_start_access(IDeckLinkVideoInputFrame *frame,
                                        IDeckLinkVideoBuffer *&buffer);
void video_input_frame_end_access(IDeckLinkVideoBuffer *buffer);
PixelFormat video_input_frame_pixel_format(IDeckLinkVideoInputFrame *frame);
BMDTimeValue video_input_frame_stream_time(IDeckLinkVideoInputFrame *frame,
                                           BMDTimeScale time_scale);

// IDeckLinkAudioInputPacket
uint8_t *audio_input_packet_bytes(IDeckLinkAudioInputPacket *input);
int64_t audio_input_packet_sample_count(IDeckLinkAudioInputPacket *input);
BMDTimeValue audio_input_packet_packet_time(IDeckLinkAudioInputPacket *input,
                                            BMDTimeScale time_scale);

// IDeckLinkDisplayMode
int64_t display_mode_width(IDeckLinkDisplayMode *mode);
int64_t display_mode_height(IDeckLinkDisplayMode *mode);
rust::String display_mode_name(IDeckLinkDisplayMode *mode);
DisplayModeType display_mode_display_mode_type(IDeckLinkDisplayMode *mode);
Ratio display_mode_frame_rate(IDeckLinkDisplayMode *mode);
void display_mode_release(IDeckLinkDisplayMode *mode);
