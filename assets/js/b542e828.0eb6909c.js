"use strict";(self.webpackChunkcompositor_live=self.webpackChunkcompositor_live||[]).push([[508],{5237:(e,t,n)=>{n.r(t),n.d(t,{assets:()=>c,contentTitle:()=>o,default:()=>p,frontMatter:()=>i,metadata:()=>d,toc:()=>a});var r=n(5893),s=n(1151);const i={description:"API routes to configure the compositor."},o="Routes",d={id:"api/routes",title:"Routes",description:"API routes to configure the compositor.",source:"@site/pages/api/routes.md",sourceDirName:"api",slug:"/api/routes",permalink:"/video_compositor/docs/api/routes",draft:!1,unlisted:!1,tags:[],version:"current",frontMatter:{description:"API routes to configure the compositor."},sidebar:"sidebar",previous:{title:"API Reference",permalink:"/video_compositor/docs/category/api-reference"},next:{title:"Streams",permalink:"/video_compositor/docs/api/io"}},c={},a=[{value:"Init",id:"init",level:3},{value:"Start",id:"start",level:3},{value:"Update scene",id:"update-scene",level:3},{value:"Register input stream",id:"register-input-stream",level:3},{value:"Register output stream",id:"register-output-stream",level:3},{value:"Register renderer",id:"register-renderer",level:3},{value:"Unregister request",id:"unregister-request",level:3}];function l(e){const t={a:"a",code:"code",h1:"h1",h3:"h3",hr:"hr",li:"li",p:"p",pre:"pre",ul:"ul",...(0,s.a)(),...e.components};return(0,r.jsxs)(r.Fragment,{children:[(0,r.jsx)(t.h1,{id:"routes",children:"Routes"}),"\n",(0,r.jsx)(t.h3,{id:"init",children:"Init"}),"\n",(0,r.jsx)(t.pre,{children:(0,r.jsx)(t.code,{className:"language-typescript",children:'Init = {\n  type: "init",\n  web_renderer: WebRendererOptions,\n  framerate: number,\n  stream_fallback_timeout_ms?: number // default: 1000\n}\n'})}),"\n",(0,r.jsx)(t.p,{children:"Init request triggers the initial setup of a compositor. It defines the base settings of a compositor that need to be evaluated before any other work happens."}),"\n",(0,r.jsxs)(t.ul,{children:["\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"web_renderer"})," - Web renderer specific options. Read more ",(0,r.jsx)(t.a,{href:"https://github.com/membraneframework/video_compositor/wiki/Web-renderer#global-options",children:"here"}),"."]}),"\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"framerate"})," - Target framerate of the output streams."]}),"\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"stream_fallback_timeout_ms"})," (default: 1000) - Timeout that defines when the compositor should switch to fallback on the input stream that stopped sending frames. See ",(0,r.jsx)(t.a,{href:"https://github.com/membraneframework/video_compositor/wiki/Main-concepts#fallback",children:"fallback"})," to learn more."]}),"\n"]}),"\n",(0,r.jsx)(t.hr,{}),"\n",(0,r.jsx)(t.h3,{id:"start",children:"Start"}),"\n",(0,r.jsx)(t.pre,{children:(0,r.jsx)(t.code,{className:"language-typescript",children:'Start = {\n  type: "start"\n}\n'})}),"\n",(0,r.jsx)(t.p,{children:"Starts the processing pipeline. If outputs are registered and defined in the scene then the compositor will start to send the RTP stream."}),"\n",(0,r.jsx)(t.hr,{}),"\n",(0,r.jsx)(t.h3,{id:"update-scene",children:"Update scene"}),"\n",(0,r.jsx)(t.pre,{children:(0,r.jsx)(t.code,{className:"language-typescript",children:'UpdateScene = {\n  type: "update_scene",\n  nodes: Array<Node>,\n  outputs: Array<Output>,\n}\n\nNode = {\n  type: "shader" | "web_renderer" | "text_renderer" | "built-in",\n  node_id: NodeId,\n  input_pads: Array<NodeId>,\n  fallback_id?: NodeId,\n  ...\n}\n\nOutput = {\n  output_id: string,\n  input_pad: NodeId,\n}\n\nNodeId = string\n'})}),"\n",(0,r.jsxs)(t.p,{children:["Update ",(0,r.jsx)(t.a,{href:"https://github.com/membraneframework/video_compositor/wiki/Main-concepts#scene",children:"scene"}),"."]}),"\n",(0,r.jsxs)(t.ul,{children:["\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"nodes"})," - List of nodes in the pipeline. Each node defines how inputs are converted into an output.","\n",(0,r.jsxs)(t.ul,{children:["\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"nodes[].node_id"})," - Id of a node."]}),"\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"nodes[].input_pads"})," - List of node ids that identify nodes needed for a current node to render. The actual meaning of that list depends on specific node implementation."]}),"\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"nodes[].fallback_id"})," - Id of a node that will be used instead of the current one if ",(0,r.jsx)(t.a,{href:"https://github.com/membraneframework/video_compositor/wiki/Main-concepts#fallback",children:"fallback"})," is triggered."]}),"\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"nodes[].*"})," - other params depend on the node type. See ",(0,r.jsx)(t.a,{href:"https://github.com/membraneframework/video_compositor/wiki/API-%E2%80%90-nodes",children:"Api - nodes"})," for more."]}),"\n"]}),"\n"]}),"\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"outputs"})," - List of outputs. Identifies which nodes should be used to produce RTP output streams.","\n",(0,r.jsxs)(t.ul,{children:["\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"outputs[].output_id"})," - Id of an already registered output stream. See ",(0,r.jsx)(t.code,{children:"RegisterOutputStream"}),"."]}),"\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"outputs[].input_pad"})," - Id of a node that will be used to produce frames for output ",(0,r.jsx)(t.code,{children:"output_id"}),"."]}),"\n"]}),"\n"]}),"\n"]}),"\n",(0,r.jsx)(t.hr,{}),"\n",(0,r.jsx)(t.h3,{id:"register-input-stream",children:"Register input stream"}),"\n",(0,r.jsx)(t.pre,{children:(0,r.jsx)(t.code,{className:"language-typescript",children:'RegisterInputStream = {\n  type: "register",\n  entity_type: "input_stream",\n  input_id: string,\n  port: number\n}\n'})}),"\n",(0,r.jsxs)(t.p,{children:["Register a new ",(0,r.jsx)(t.a,{href:"https://github.com/membraneframework/video_compositor/wiki/Main-concepts#inputoutput-streams",children:"input stream"}),"."]}),"\n",(0,r.jsxs)(t.ul,{children:["\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"input_id"})," - Identifier that can be used in ",(0,r.jsx)(t.code,{children:"UpdateScene"})," request to connect that stream to transformations or outputs."]}),"\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"port"})," - UDP port that the compositor should listen for stream."]}),"\n"]}),"\n",(0,r.jsx)(t.hr,{}),"\n",(0,r.jsx)(t.h3,{id:"register-output-stream",children:"Register output stream"}),"\n",(0,r.jsx)(t.pre,{children:(0,r.jsx)(t.code,{className:"language-typescript",children:'RegisterOutputStream = {\n  type: "register",\n  entity_type: "output_stream",\n  output_id: string,\n  port: number,\n  ip: string,\n  resolution: {\n    width: number,\n    height: number,\n  },\n  encoder_settings: {\n    preset: EncoderPreset\n  }\n}\n\nEncoderPreset =\n  | "ultrafast"\n  | "superfast"\n  | "veryfast"\n  | "faster"\n  | "fast"\n  | "medium"\n  | "slow"\n  | "slower"\n  | "veryslow"\n  | "placebo"\n'})}),"\n",(0,r.jsxs)(t.p,{children:["Register a new ",(0,r.jsx)(t.a,{href:"https://github.com/membraneframework/video_compositor/wiki/Main-concepts#inputoutput-streams",children:"output stream"}),"."]}),"\n",(0,r.jsxs)(t.ul,{children:["\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"output_id"})," - Identifier that can be used in ",(0,r.jsx)(t.code,{children:"UpdateScene"})," request to assign a node that will be used to produce frames for a stream."]}),"\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"port"})," / ",(0,r.jsx)(t.code,{children:"ip"})," - UDP port and IP where compositor should send the stream."]}),"\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"resolution"})," - Output resolution in pixels."]}),"\n",(0,r.jsxs)(t.li,{children:[(0,r.jsx)(t.code,{children:"encoder_settings.preset"})," - Preset for an encoder. See ",(0,r.jsx)(t.code,{children:"FFmpeg"})," ",(0,r.jsx)(t.a,{href:"https://trac.ffmpeg.org/wiki/Encode/H.264#Preset",children:"docs"})," to learn more."]}),"\n"]}),"\n",(0,r.jsx)(t.h3,{id:"register-renderer",children:"Register renderer"}),"\n",(0,r.jsx)(t.pre,{children:(0,r.jsx)(t.code,{className:"language-typescript",children:'RegisterRenderer = {\n  type: "register",\n  entity_type: "shader" | "web_renderer" | "image",\n  ... // renderer specific options\n}\n'})}),"\n",(0,r.jsxs)(t.p,{children:["See ",(0,r.jsx)(t.a,{href:"https://github.com/membraneframework/video_compositor/wiki/Api-%E2%80%90-renderers",children:"renderer documentation"})," to learn more."]}),"\n",(0,r.jsx)(t.h3,{id:"unregister-request",children:"Unregister request"}),"\n",(0,r.jsx)(t.pre,{children:(0,r.jsx)(t.code,{className:"language-typescript",children:'Unregister =\n  | { entity_type: "input_stream", input_id: string }\n  | { entity_type: "output_stream", output_id: string }\n  | { entity_type: "shader", shader_id: string }\n  | { entity_type: "image", image_id: string }\n  | { entity_type: "web_renderer", instance_id: string };\n'})})]})}function p(e={}){const{wrapper:t}={...(0,s.a)(),...e.components};return t?(0,r.jsx)(t,{...e,children:(0,r.jsx)(l,{...e})}):l(e)}},1151:(e,t,n)=>{n.d(t,{Z:()=>d,a:()=>o});var r=n(7294);const s={},i=r.createContext(s);function o(e){const t=r.useContext(i);return r.useMemo((function(){return"function"==typeof e?e(t):{...t,...e}}),[t,e])}function d(e){let t;return t=e.disableParentContext?"function"==typeof e.components?e.components(s):e.components||s:o(e.components),r.createElement(i.Provider,{value:t},e.children)}}}]);