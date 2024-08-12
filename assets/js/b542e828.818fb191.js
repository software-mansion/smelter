"use strict";(self.webpackChunkcompositor_live=self.webpackChunkcompositor_live||[]).push([[7508],{5237:(e,n,i)=>{i.r(n),i.d(n,{assets:()=>a,contentTitle:()=>d,default:()=>u,frontMatter:()=>t,metadata:()=>c,toc:()=>o});var r=i(5893),s=i(1151);const t={description:"API routes to configure the compositor."},d="Routes",c={id:"api/routes",title:"Routes",description:"API routes to configure the compositor.",source:"@site/pages/api/routes.md",sourceDirName:"api",slug:"/api/routes",permalink:"/docs/api/routes",draft:!1,unlisted:!1,tags:[],version:"current",frontMatter:{description:"API routes to configure the compositor."},sidebar:"sidebar",previous:{title:"API Reference",permalink:"/docs/category/api-reference"},next:{title:"Events",permalink:"/docs/api/events"}},a={},o=[{value:"Start request",id:"start-request",level:2},{value:"Outputs configuration",id:"outputs-configuration",level:2},{value:"Register output",id:"register-output",level:3},{value:"Unregister output",id:"unregister-output",level:3},{value:"Update output",id:"update-output",level:3},{value:"Request keyframe",id:"request-keyframe",level:3},{value:"Inputs configuration",id:"inputs-configuration",level:2},{value:"Register input",id:"register-input",level:3},{value:"Unregister input",id:"unregister-input",level:3},{value:"Renderers configuration",id:"renderers-configuration",level:2},{value:"Register image",id:"register-image",level:3},{value:"Unregister image",id:"unregister-image",level:3},{value:"Register shader",id:"register-shader",level:3},{value:"Unregister shader",id:"unregister-shader",level:3},{value:"Register web renderer instance",id:"register-web-renderer-instance",level:3},{value:"Unregister web renderer instance",id:"unregister-web-renderer-instance",level:3},{value:"Status endpoint",id:"status-endpoint",level:2},{value:"WebSocket endpoint",id:"websocket-endpoint",level:2}];function l(e){const n={a:"a",code:"code",h1:"h1",h2:"h2",h3:"h3",hr:"hr",li:"li",p:"p",pre:"pre",strong:"strong",ul:"ul",...(0,s.a)(),...e.components};return(0,r.jsxs)(r.Fragment,{children:[(0,r.jsx)(n.h1,{id:"routes",children:"Routes"}),"\n",(0,r.jsxs)(n.p,{children:["API is served by default on the port 8081. Different port can be configured using ",(0,r.jsx)(n.a,{href:"../deployment/configuration#live_compositor_api_port",children:(0,r.jsx)(n.code,{children:"LIVE_COMPOSITOR_API_PORT"})})," environment variable."]}),"\n",(0,r.jsx)(n.h2,{id:"start-request",children:"Start request"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/start\n"})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-typescript",children:"type RequestBody = {}\n"})}),"\n",(0,r.jsx)(n.p,{children:"Starts the processing pipeline. If outputs are registered and defined in the scene then the compositor will start to send the RTP streams."}),"\n",(0,r.jsx)(n.hr,{}),"\n",(0,r.jsx)(n.h2,{id:"outputs-configuration",children:"Outputs configuration"}),"\n",(0,r.jsx)(n.h3,{id:"register-output",children:"Register output"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/output/:output_id/register\n"})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-typescript",children:'type RequestBody = {\n  type: "rtp_stream" | "mp4"\n  ... // output specific options\n}\n'})}),"\n",(0,r.jsx)(n.p,{children:"Register external destination that can be used as a compositor output. See outputs documentation to learn more."}),"\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsx)(n.li,{children:(0,r.jsx)(n.a,{href:"/docs/api/outputs/rtp",children:"RTP"})}),"\n",(0,r.jsx)(n.li,{children:(0,r.jsx)(n.a,{href:"/docs/api/outputs/mp4",children:"MP4"})}),"\n"]}),"\n",(0,r.jsx)(n.h3,{id:"unregister-output",children:"Unregister output"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST /api/output/:output_id/unregister\n"})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-typescript",children:"type RequestBody = {\n  schedule_time_ms?: number;\n}\n"})}),"\n",(0,r.jsxs)(n.p,{children:["Unregister a previously registered output with an id ",(0,r.jsx)(n.code,{children:":output_id"}),"."]}),"\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"schedule_time_ms"})," - Time in milliseconds when this request should be applied. Value ",(0,r.jsx)(n.code,{children:"0"})," represents time of ",(0,r.jsx)(n.a,{href:"#start-request",children:"the start request"}),"."]}),"\n"]}),"\n",(0,r.jsx)(n.h3,{id:"update-output",children:"Update output"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/output/:output_id/update\n"})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-typescript",children:"type RequestBody = {\n  video?: {\n    root: Component\n  };\n  audio?: {\n    inputs: AudioInput[];\n  };\n  schedule_time_ms?: number;\n}\n\ntype AudioInput = {\n  input_id: InputId;\n  volume?: number;\n}\n"})}),"\n",(0,r.jsxs)(n.p,{children:["Update scene definition and audio mixer configuration for output with ID ",(0,r.jsx)(n.code,{children:":output_id"}),". The output stream has to be registered first. See ",(0,r.jsx)(n.a,{href:"/docs/api/routes#register-output",children:(0,r.jsx)(n.code,{children:"register output"})})," request."]}),"\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"video"})," - Configuration for video output."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"video.root"})," - Root of a component tree/scene that should be rendered for the output. ",(0,r.jsx)(n.a,{href:"../concept/component",children:"Learn more"})]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"audio"})," - Configuration for audio output."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"audio.inputs"})," - Input streams that should be mixed together and their configuration."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"audio.inputs[].input_id"})," - Input ID."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"audio.inputs[].volume"})," - (",(0,r.jsxs)(n.strong,{children:["default=",(0,r.jsx)(n.code,{children:"1.0"})]}),") Float in ",(0,r.jsx)(n.code,{children:"[0, 1]"})," range representing volume."]}),"\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"schedule_time_ms"})," - Time in milliseconds when this request should be applied. Value ",(0,r.jsx)(n.code,{children:"0"})," represents time of ",(0,r.jsx)(n.a,{href:"#start-request",children:"the start request"}),"."]}),"\n"]}),"\n",(0,r.jsx)(n.hr,{}),"\n",(0,r.jsx)(n.h3,{id:"request-keyframe",children:"Request keyframe"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/output/:output_id/request_keyframe\n"})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-typescript",children:"type RequestBody = {}\n"})}),"\n",(0,r.jsx)(n.p,{children:"Requests additional keyframe (I frame) on the video output."}),"\n",(0,r.jsx)(n.h2,{id:"inputs-configuration",children:"Inputs configuration"}),"\n",(0,r.jsx)(n.h3,{id:"register-input",children:"Register input"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/input/:input_id/register\n"})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-typescript",children:'type RequestBody = {\n  type: "rtp_stream" | "mp4" | "decklink";\n  ... // input specific options\n}\n'})}),"\n",(0,r.jsx)(n.p,{children:"Register external source that can be used as a compositor input. See inputs documentation to learn more."}),"\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsx)(n.li,{children:(0,r.jsx)(n.a,{href:"/docs/api/inputs/rtp",children:"RTP"})}),"\n",(0,r.jsx)(n.li,{children:(0,r.jsx)(n.a,{href:"/docs/api/inputs/mp4",children:"MP4"})}),"\n",(0,r.jsx)(n.li,{children:(0,r.jsx)(n.a,{href:"/docs/api/inputs/decklink",children:"DeckLink"})}),"\n"]}),"\n",(0,r.jsx)(n.h3,{id:"unregister-input",children:"Unregister input"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/input/:input_id/unregister\n"})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-typescript",children:"type RequestBody = {\n  schedule_time_ms?: number;\n}\n"})}),"\n",(0,r.jsxs)(n.p,{children:["Unregister a previously registered input with an id ",(0,r.jsx)(n.code,{children:":input_id"}),"."]}),"\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"schedule_time_ms"})," - Time in milliseconds when this request should be applied. Value ",(0,r.jsx)(n.code,{children:"0"})," represents time of ",(0,r.jsx)(n.a,{href:"#start-request",children:"the start request"}),"."]}),"\n"]}),"\n",(0,r.jsx)(n.hr,{}),"\n",(0,r.jsx)(n.h2,{id:"renderers-configuration",children:"Renderers configuration"}),"\n",(0,r.jsx)(n.h3,{id:"register-image",children:"Register image"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/image/:image_id/register\n"})}),"\n",(0,r.jsxs)(n.p,{children:["Register an image asset. Request body is defined in the ",(0,r.jsx)(n.a,{href:"/docs/api/renderers/image",children:"image"})," docs."]}),"\n",(0,r.jsx)(n.h3,{id:"unregister-image",children:"Unregister image"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/image/:image_id/unregister\n"})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-typescript",children:"type RequestBody = {}\n"})}),"\n",(0,r.jsxs)(n.p,{children:["Unregister a previously registered image asset with an id ",(0,r.jsx)(n.code,{children:":image_id"}),"."]}),"\n",(0,r.jsx)(n.h3,{id:"register-shader",children:"Register shader"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/shader/:shader_id/register\n"})}),"\n",(0,r.jsxs)(n.p,{children:["Register a shader. Request body is defined in the ",(0,r.jsx)(n.a,{href:"/docs/api/renderers/shader",children:"shader"})," docs."]}),"\n",(0,r.jsx)(n.h3,{id:"unregister-shader",children:"Unregister shader"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/shader/:shader_id/unregister\n"})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-typescript",children:"type RequestBody = {}\n"})}),"\n",(0,r.jsxs)(n.p,{children:["Unregister a previously registered shader with an id ",(0,r.jsx)(n.code,{children:":shader_id"}),"."]}),"\n",(0,r.jsx)(n.h3,{id:"register-web-renderer-instance",children:"Register web renderer instance"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/web-renderer/:instance_id/register\n"})}),"\n",(0,r.jsxs)(n.p,{children:["Register a web renderer instance. Request body is defined in the ",(0,r.jsx)(n.a,{href:"/docs/api/renderers/web",children:"web renderer"})," docs."]}),"\n",(0,r.jsx)(n.h3,{id:"unregister-web-renderer-instance",children:"Unregister web renderer instance"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"POST: /api/web-renderer/:instance_id/unregister\n"})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-typescript",children:"type RequestBody = {}\n"})}),"\n",(0,r.jsxs)(n.p,{children:["Unregister a previously registered web renderer instance with an id ",(0,r.jsx)(n.code,{children:":instance_id"}),"."]}),"\n",(0,r.jsx)(n.h2,{id:"status-endpoint",children:"Status endpoint"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"GET: /status\n"})}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-typescript",children:"type Response = {\n  instance_id: string\n}\n"})}),"\n",(0,r.jsxs)(n.p,{children:["Status/health check endpoint. Returns ",(0,r.jsx)(n.code,{children:"200 OK"}),"."]}),"\n",(0,r.jsxs)(n.ul,{children:["\n",(0,r.jsxs)(n.li,{children:[(0,r.jsx)(n.code,{children:"instance_id"})," - ID that can be provided using ",(0,r.jsx)(n.code,{children:"LIVE_COMPOSITOR_INSTANCE_ID"})," environment variable. Defaults to random value in the format ",(0,r.jsx)(n.code,{children:"live_compositor_{RANDOM_VALUE}"}),"."]}),"\n"]}),"\n",(0,r.jsx)(n.h2,{id:"websocket-endpoint",children:"WebSocket endpoint"}),"\n",(0,r.jsx)(n.pre,{children:(0,r.jsx)(n.code,{className:"language-http",children:"/ws\n"})}),"\n",(0,r.jsxs)(n.p,{children:["Establish WebSocket connection to listen for LiveCompositor events. List of supported events and their descriptions can be found ",(0,r.jsx)(n.a,{href:"/docs/api/events",children:"here"}),"."]})]})}function u(e={}){const{wrapper:n}={...(0,s.a)(),...e.components};return n?(0,r.jsx)(n,{...e,children:(0,r.jsx)(l,{...e})}):l(e)}},1151:(e,n,i)=>{i.d(n,{Z:()=>c,a:()=>d});var r=i(7294);const s={},t=r.createContext(s);function d(e){const n=r.useContext(t);return r.useMemo((function(){return"function"==typeof e?e(n):{...n,...e}}),[n,e])}function c(e){let n;return n=e.disableParentContext?"function"==typeof e.components?e.components(s):e.components||s:d(e.components),r.createElement(t.Provider,{value:n},e.children)}}}]);