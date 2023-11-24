"use strict";(self.webpackChunkcompositor_live=self.webpackChunkcompositor_live||[]).push([[53],{1109:e=>{e.exports=JSON.parse('{"pluginId":"default","version":"current","label":"Next","banner":null,"badge":false,"noIndex":false,"className":"docs-version-current","isLast":true,"docsSidebars":{"sidebar":[{"type":"link","label":"Introduction","href":"/video_compositor/docs/intro","docId":"intro","unlisted":false},{"label":"Get started","type":"category","items":[{"type":"link","label":"Elixir","href":"/video_compositor/docs/get-started/elixir","docId":"get-started/elixir","unlisted":false},{"type":"link","label":"Node.js","href":"/video_compositor/docs/get-started/node","docId":"get-started/node","unlisted":false}],"collapsed":true,"collapsible":true,"href":"/video_compositor/docs/get-started"},{"type":"category","label":"API Reference","collapsible":false,"items":[{"type":"link","label":"HTTP Routes","href":"/video_compositor/docs/api/routes","docId":"api/routes","unlisted":false},{"type":"link","label":"Input/Output streams","href":"/video_compositor/docs/api/io","docId":"api/io","unlisted":false},{"type":"category","label":"Components","collapsible":false,"description":"Basic blocks used to define a scene.","items":[{"type":"link","label":"Shader","href":"/video_compositor/docs/api/components/shader","docId":"api/components/shader","unlisted":false},{"type":"link","label":"WebView","href":"/video_compositor/docs/api/components/web","docId":"api/components/web","unlisted":false}],"collapsed":false},{"type":"category","label":"Renderers","collapsible":false,"description":"Resources that need to be registered first before they can be used.","items":[{"type":"link","label":"Shader","href":"/video_compositor/docs/api/renderers/shader","docId":"api/renderers/shader","unlisted":false}],"collapsed":false}],"collapsed":false,"href":"/video_compositor/docs/category/api-reference"}]},"docs":{"api/api":{"id":"api/api","title":"API","description":"Compositor exposes HTTP API. After spawning a compositor process the first request has to be an init request. After the compositor is initialized you can configure the processing pipeline using RegisterInputStream, RegisterOutputStre, UpdateScene, and other requests. When you are ready to start receiving the output stream from the compositor you can send a Start request."},"api/components/shader":{"id":"api/components/shader","title":"Shader","description":"See shader documentation to learn more.","sidebar":"sidebar"},"api/components/web":{"id":"api/components/web","title":"WebView","description":"See web renderer documentation to learn more.","sidebar":"sidebar"},"api/io":{"id":"api/io","title":"Streams","description":"Configuration and delivery of input and output streams.","sidebar":"sidebar"},"api/renderers/shader":{"id":"api/renderers/shader","title":"Shader","description":"","sidebar":"sidebar"},"api/routes":{"id":"api/routes","title":"Routes","description":"API routes to configure the compositor.","sidebar":"sidebar"},"get-started":{"id":"get-started","title":"Get started","description":"To familiarize yourself with a compositor you can start with examples directory. It includes example applications that use ffmpeg and ffplay to simulate compositor inputs and outputs. For a more detailed explanation of some of the terms used in this documentation, you can check this page.","sidebar":"sidebar"},"get-started/elixir":{"id":"get-started/elixir","title":"Elixir","description":"See Membrane Live Compositor plugin for more.","sidebar":"sidebar"},"get-started/node":{"id":"get-started/node","title":"Node.js","description":"See github.com/membraneframework-labs/rtconvideocompositorworkshops for example usage.","sidebar":"sidebar"},"intro":{"id":"intro","title":"Introduction","description":"Live compositor is an application for real-time video processing/transforming/composing, providing simple, language-agnostic API for live video rendering. It targets real-time use cases, like video conferencing, live-streaming, or broadcasting (e.g. with WebRTC / HLS / RTMP).","sidebar":"sidebar"}}}')}}]);