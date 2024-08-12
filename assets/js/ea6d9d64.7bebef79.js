"use strict";(self.webpackChunkcompositor_live=self.webpackChunkcompositor_live||[]).push([[1897],{7786:(e,n,t)=>{t.r(n),t.d(n,{assets:()=>p,contentTitle:()=>r,default:()=>c,frontMatter:()=>o,metadata:()=>d,toc:()=>l});var i=t(5893),s=t(1151);const o={},r=void 0,d={id:"api/generated/output-Mp4Output",title:"output-Mp4Output",description:"Mp4Output",source:"@site/pages/api/generated/output-Mp4Output.md",sourceDirName:"api/generated",slug:"/api/generated/output-Mp4Output",permalink:"/docs/api/generated/output-Mp4Output",draft:!1,unlisted:!1,tags:[],version:"current",frontMatter:{}},p={},l=[{value:"Mp4Output",id:"mp4output",level:2},{value:"Properties",id:"properties",level:4},{value:"OutputVideoOptions",id:"outputvideooptions",level:2},{value:"Properties",id:"properties-1",level:4},{value:"OutputMp4AudioOptions",id:"outputmp4audiooptions",level:2},{value:"Properties",id:"properties-2",level:4},{value:"OutputEndCondition",id:"outputendcondition",level:2},{value:"Properties",id:"properties-3",level:4},{value:"VideoEncoderOptions",id:"videoencoderoptions",level:2},{value:"Properties (<code>type: &quot;ffmpeg_h264&quot;</code>)",id:"properties-type-ffmpeg_h264",level:4},{value:"Mp4AudioEncoderOptions",id:"mp4audioencoderoptions",level:2},{value:"InputAudio",id:"inputaudio",level:2},{value:"Properties",id:"properties-4",level:4}];function u(e){const n={a:"a",code:"code",h2:"h2",h4:"h4",li:"li",p:"p",pre:"pre",strong:"strong",ul:"ul",...(0,s.a)(),...e.components};return(0,i.jsxs)(i.Fragment,{children:[(0,i.jsx)(n.h2,{id:"mp4output",children:"Mp4Output"}),"\n",(0,i.jsx)(n.pre,{children:(0,i.jsx)(n.code,{className:"language-typescript",children:"type Mp4Output = {\n  path: string;\n  video?: OutputVideoOptions;\n  audio?: OutputMp4AudioOptions;\n}\n"})}),"\n",(0,i.jsx)(n.h4,{id:"properties",children:"Properties"}),"\n",(0,i.jsxs)(n.ul,{children:["\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"path"})," - Path to output MP4 file."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"video"})," - Video track configuration."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"audio"})," - Audio track configuration."]}),"\n"]}),"\n",(0,i.jsx)(n.h2,{id:"outputvideooptions",children:"OutputVideoOptions"}),"\n",(0,i.jsx)(n.pre,{children:(0,i.jsx)(n.code,{className:"language-typescript",children:"type OutputVideoOptions = {\n  resolution: {\n    width: u32;\n    height: u32;\n  };\n  send_eos_when?: OutputEndCondition;\n  encoder: VideoEncoderOptions;\n  initial: { root: Component; };\n}\n"})}),"\n",(0,i.jsx)(n.h4,{id:"properties-1",children:"Properties"}),"\n",(0,i.jsxs)(n.ul,{children:["\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"resolution"})," - Output resolution in pixels."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"send_eos_when"})," - Defines when output stream should end if some of the input streams are finished. If output includes both audio and video streams, then EOS needs to be sent on both."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"encoder"})," - Video encoder options."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"initial"})," - Root of a component tree/scene that should be rendered for the output. Use ",(0,i.jsxs)(n.a,{href:"/docs/api/routes#update-output",children:[(0,i.jsx)(n.code,{children:"update_output"})," request"]})," to update this value after registration. ",(0,i.jsx)(n.a,{href:"/docs/concept/component",children:"Learn more"}),"."]}),"\n"]}),"\n",(0,i.jsx)(n.h2,{id:"outputmp4audiooptions",children:"OutputMp4AudioOptions"}),"\n",(0,i.jsx)(n.pre,{children:(0,i.jsx)(n.code,{className:"language-typescript",children:'type OutputMp4AudioOptions = {\n  mixing_strategy?: "sum_clip" | "sum_scale";\n  send_eos_when?: OutputEndCondition;\n  encoder: Mp4AudioEncoderOptions;\n  initial: { inputs: InputAudio[]; };\n}\n'})}),"\n",(0,i.jsx)(n.h4,{id:"properties-2",children:"Properties"}),"\n",(0,i.jsxs)(n.ul,{children:["\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"mixing_strategy"})," - (",(0,i.jsx)(n.strong,{children:'default="sum_clip"'}),") Specifies how audio should be mixed.","\n",(0,i.jsxs)(n.ul,{children:["\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:'"sum_clip"'})," - Firstly, input samples are summed. If the result is outside the i16 PCM range, it gets clipped."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:'"sum_scale"'})," - Firstly, input samples are summed. If the result is outside the i16 PCM range,\nnearby summed samples are scaled down by factor, such that the summed wave is in the i16 PCM range."]}),"\n"]}),"\n"]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"send_eos_when"})," - Condition for termination of output stream based on the input streams states."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"encoder"})," - Audio encoder options."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"initial"})," - Initial audio mixer configuration for output."]}),"\n"]}),"\n",(0,i.jsx)(n.h2,{id:"outputendcondition",children:"OutputEndCondition"}),"\n",(0,i.jsx)(n.pre,{children:(0,i.jsx)(n.code,{className:"language-typescript",children:"type OutputEndCondition = {\n  any_of?: string[];\n  all_of?: string[];\n  any_input?: bool;\n  all_inputs?: bool;\n}\n"})}),"\n",(0,i.jsx)(n.p,{children:"This type defines when end of an input stream should trigger end of the output stream. Only one of those fields can be set at the time.\nUnless specified otherwise the input stream is considered finished/ended when:"}),"\n",(0,i.jsxs)(n.ul,{children:["\n",(0,i.jsx)(n.li,{children:"TCP connection was dropped/closed."}),"\n",(0,i.jsxs)(n.li,{children:["RTCP Goodbye packet (",(0,i.jsx)(n.code,{children:"BYE"}),") was received."]}),"\n",(0,i.jsx)(n.li,{children:"Mp4 track has ended."}),"\n",(0,i.jsx)(n.li,{children:"Input was unregistered already (or never registered)."}),"\n"]}),"\n",(0,i.jsx)(n.h4,{id:"properties-3",children:"Properties"}),"\n",(0,i.jsxs)(n.ul,{children:["\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"any_of"})," - Terminate output stream if any of the input streams from the list are finished."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"all_of"})," - Terminate output stream if all the input streams from the list are finished."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"any_input"})," - Terminate output stream if any of the input streams ends. This includes streams added after the output was registered. In particular, output stream will ",(0,i.jsx)(n.strong,{children:"not be"})," terminated if no inputs were ever connected."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"all_inputs"})," - Terminate output stream if all the input streams finish. In particular, output stream will ",(0,i.jsx)(n.strong,{children:"be"})," terminated if no inputs were ever connected."]}),"\n"]}),"\n",(0,i.jsx)(n.h2,{id:"videoencoderoptions",children:"VideoEncoderOptions"}),"\n",(0,i.jsx)(n.pre,{children:(0,i.jsx)(n.code,{className:"language-typescript",children:'type VideoEncoderOptions = \n  | {\n      type: "ffmpeg_h264";\n      preset: \n        | "ultrafast"\n        | "superfast"\n        | "veryfast"\n        | "faster"\n        | "fast"\n        | "medium"\n        | "slow"\n        | "slower"\n        | "veryslow"\n        | "placebo";\n      ffmpeg_options?: Map<string, string>;\n    }\n'})}),"\n",(0,i.jsxs)(n.h4,{id:"properties-type-ffmpeg_h264",children:["Properties (",(0,i.jsx)(n.code,{children:'type: "ffmpeg_h264"'}),")"]}),"\n",(0,i.jsxs)(n.ul,{children:["\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"preset"})," - (",(0,i.jsxs)(n.strong,{children:["default=",(0,i.jsx)(n.code,{children:'"fast"'})]}),") Preset for an encoder. See ",(0,i.jsx)(n.code,{children:"FFmpeg"})," ",(0,i.jsx)(n.a,{href:"https://trac.ffmpeg.org/wiki/Encode/H.264#Preset",children:"docs"})," to learn more."]}),"\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"ffmpeg_options"})," - Raw FFmpeg encoder options. See ",(0,i.jsx)(n.a,{href:"https://ffmpeg.org/ffmpeg-codecs.html",children:"docs"})," for more."]}),"\n"]}),"\n",(0,i.jsx)(n.h2,{id:"mp4audioencoderoptions",children:"Mp4AudioEncoderOptions"}),"\n",(0,i.jsx)(n.pre,{children:(0,i.jsx)(n.code,{className:"language-typescript",children:'type Mp4AudioEncoderOptions = { type: "aac"; channels: "mono" | "stereo"; }\n'})}),"\n",(0,i.jsx)(n.h2,{id:"inputaudio",children:"InputAudio"}),"\n",(0,i.jsx)(n.pre,{children:(0,i.jsx)(n.code,{className:"language-typescript",children:"type InputAudio = {\n  input_id: string;\n  volume?: f32;\n}\n"})}),"\n",(0,i.jsx)(n.h4,{id:"properties-4",children:"Properties"}),"\n",(0,i.jsxs)(n.ul,{children:["\n",(0,i.jsxs)(n.li,{children:[(0,i.jsx)(n.code,{children:"volume"})," - (",(0,i.jsxs)(n.strong,{children:["default=",(0,i.jsx)(n.code,{children:"1.0"})]}),") float in ",(0,i.jsx)(n.code,{children:"[0, 1]"})," range representing input volume"]}),"\n"]})]})}function c(e={}){const{wrapper:n}={...(0,s.a)(),...e.components};return n?(0,i.jsx)(n,{...e,children:(0,i.jsx)(u,{...e})}):u(e)}},1151:(e,n,t)=>{t.d(n,{Z:()=>d,a:()=>r});var i=t(7294);const s={},o=i.createContext(s);function r(e){const n=i.useContext(o);return i.useMemo((function(){return"function"==typeof e?e(n):{...n,...e}}),[n,e])}function d(e){let n;return n=e.disableParentContext?"function"==typeof e.components?e.components(s):e.components||s:r(e.components),i.createElement(o.Provider,{value:n},e.children)}}}]);