#pragma once

#include "api.h"
#include <atomic>
#include <cstdint>
#include <cstring>

template <typename Interface> class ComObject : public Interface {
private:
  REFIID iid;
  std::atomic<ULONG> refcount = 1;

public:
  ComObject(REFIID iid) : iid(iid){};

  virtual HRESULT STDMETHODCALLTYPE QueryInterface(REFIID requested,
                                                   LPVOID *out) {
    REFIID unknown = IID_IUnknown;
    if (memcmp(&requested, &iid, sizeof(REFIID)) != 0 &&
        memcmp(&requested, &unknown, sizeof(REFIID)) != 0) {
      *out = nullptr;
      return E_NOINTERFACE;
    }
    AddRef();
    *out = static_cast<Interface *>(this);
    return S_OK;
  }

  virtual ULONG STDMETHODCALLTYPE AddRef(void) { return ++refcount; }

  virtual ULONG STDMETHODCALLTYPE Release(void) {
    ULONG remaining = --refcount;
    if (remaining == 0) {
      delete this;
    }
    return remaining;
  }

protected:
  virtual ~ComObject() = default;
};

class InputCallbackWrapper
    : public ComObject<IDeckLinkInputCallback> {
private:
  rust::Box<DynInputCallback> cb;

public:
  InputCallbackWrapper(rust::Box<DynInputCallback> cb)
      : ComObject(IID_IDeckLinkInputCallback), cb(std::move(cb)){};

  virtual HRESULT STDMETHODCALLTYPE
  VideoInputFrameArrived(IDeckLinkVideoInputFrame *video_frame,
                         IDeckLinkAudioInputPacket *audio_packet);
  virtual HRESULT STDMETHODCALLTYPE
  VideoInputFormatChanged(BMDVideoInputFormatChangedEvents events,
                          IDeckLinkDisplayMode *display_mode,
                          BMDDetectedVideoInputFormatFlags flags);
};

// Answered by buffers a Rust `FrameAllocator` lent, so frames hand them back.
static const REFIID IID_RustFrameBuffer = {0x6D, 0x3A, 0x1F, 0x52, 0xB8, 0x04,
                                           0x4C, 0x9E, 0xA1, 0x77, 0x2E, 0x90,
                                           0x5B, 0xC3, 0x48, 0xD6};

class FrameAllocatorProvider
    : public ComObject<IDeckLinkVideoBufferAllocatorProvider> {
public:
  rust::Box<DynFrameAllocator> allocator;

  FrameAllocatorProvider(rust::Box<DynFrameAllocator> allocator)
      : ComObject(IID_IDeckLinkVideoBufferAllocatorProvider),
        allocator(std::move(allocator)){};

  HRESULT GetVideoBufferAllocator(uint32_t buffer_size, uint32_t, uint32_t,
                                  uint32_t row_bytes, BMDPixelFormat,
                                  IDeckLinkVideoBufferAllocator **out) override;
};

class FrameBuffer : public ComObject<IDeckLinkVideoBuffer> {
public:
  rust::Box<DynFrameBuffer> buffer;

  FrameBuffer(rust::Box<DynFrameBuffer> buffer)
      : ComObject(IID_IDeckLinkVideoBuffer), buffer(std::move(buffer)){};

  HRESULT STDMETHODCALLTYPE QueryInterface(REFIID requested,
                                           LPVOID *out) override {
    if (memcmp(&requested, &IID_RustFrameBuffer, sizeof(REFIID)) == 0) {
      AddRef();
      *out = this;
      return S_OK;
    }
    return ComObject::QueryInterface(requested, out);
  }

  HRESULT GetBytes(void **out) override {
    *out = buffer->bytes();
    return S_OK;
  }
  HRESULT StartAccess(BMDBufferAccessFlags) override { return S_OK; }
  HRESULT EndAccess(BMDBufferAccessFlags) override { return S_OK; }
};

class FrameBufferAllocator
    : public ComObject<IDeckLinkVideoBufferAllocator> {
private:
  FrameAllocatorProvider *provider;
  uint32_t buffer_size;
  uint32_t row_bytes;

public:
  FrameBufferAllocator(FrameAllocatorProvider *provider, uint32_t buffer_size,
                       uint32_t row_bytes)
      : ComObject(IID_IDeckLinkVideoBufferAllocator), provider(provider),
        buffer_size(buffer_size), row_bytes(row_bytes) {
    provider->AddRef();
  }

  ~FrameBufferAllocator() { provider->Release(); }

  HRESULT AllocateVideoBuffer(IDeckLinkVideoBuffer **out) override {
    DynFrameBuffer *buffer =
        provider->allocator->allocate(buffer_size, row_bytes);
    if (buffer == nullptr) {
      return E_OUTOFMEMORY;
    }
    *out = new FrameBuffer(rust::Box<DynFrameBuffer>::from_raw(buffer));
    return S_OK;
  }
};

inline HRESULT FrameAllocatorProvider::GetVideoBufferAllocator(
    uint32_t buffer_size, uint32_t, uint32_t, uint32_t row_bytes,
    BMDPixelFormat, IDeckLinkVideoBufferAllocator **out) {
  *out = new FrameBufferAllocator(this, buffer_size, row_bytes);
  return S_OK;
}
