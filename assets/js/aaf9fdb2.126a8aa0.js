"use strict";(self.webpackChunkcompositor_live=self.webpackChunkcompositor_live||[]).push([[3569],{7007:(e,r,n)=>{n.r(r),n.d(r,{assets:()=>l,contentTitle:()=>d,default:()=>h,frontMatter:()=>i,metadata:()=>s,toc:()=>o});var t=n(5893),a=n(1151);const i={},d=void 0,s={id:"api/generated/component-Shader",title:"component-Shader",description:"Shader",source:"@site/pages/api/generated/component-Shader.md",sourceDirName:"api/generated",slug:"/api/generated/component-Shader",permalink:"/docs/api/generated/component-Shader",draft:!1,unlisted:!1,tags:[],version:"current",frontMatter:{}},l={},o=[{value:"Shader",id:"shader",level:2},{value:"Properties",id:"properties",level:4},{value:"ShaderParam",id:"shaderparam",level:2},{value:"ShaderParamStructField",id:"shaderparamstructfield",level:2}];function c(e){const r={a:"a",admonition:"admonition",code:"code",h2:"h2",h4:"h4",li:"li",p:"p",pre:"pre",ul:"ul",...(0,a.a)(),...e.components};return(0,t.jsxs)(t.Fragment,{children:[(0,t.jsx)(r.h2,{id:"shader",children:"Shader"}),"\n",(0,t.jsx)(r.pre,{children:(0,t.jsx)(r.code,{className:"language-typescript",children:"type Shader = {\n  id?: string;\n  children?: Component[];\n  shader_id: string;\n  shader_param?: ShaderParam;\n  resolution: { width: u32; height: u32 };\n}\n"})}),"\n",(0,t.jsx)(r.h4,{id:"properties",children:"Properties"}),"\n",(0,t.jsxs)(r.ul,{children:["\n",(0,t.jsxs)(r.li,{children:[(0,t.jsx)(r.code,{children:"id"})," - Id of a component."]}),"\n",(0,t.jsxs)(r.li,{children:[(0,t.jsx)(r.code,{children:"children"})," - List of component's children."]}),"\n",(0,t.jsxs)(r.li,{children:[(0,t.jsx)(r.code,{children:"shader_id"})," - Id of a shader. It identifies a shader registered using a ",(0,t.jsx)(r.a,{href:"/docs/api/routes#register-shader",children:(0,t.jsx)(r.code,{children:"register shader"})})," request."]}),"\n",(0,t.jsxs)(r.li,{children:[(0,t.jsx)(r.code,{children:"shader_param"})," - Object that will be serialized into a ",(0,t.jsx)(r.code,{children:"struct"})," and passed inside the shader as:","\n",(0,t.jsx)("br",{}),"\n",(0,t.jsx)("br",{}),"\n",(0,t.jsx)(r.pre,{children:(0,t.jsx)(r.code,{className:"language-wgsl",children:"@group(1) @binding(0) var<uniform>\n"})}),"\n",(0,t.jsx)(r.admonition,{type:"note",children:(0,t.jsxs)(r.p,{children:["This object's structure must match the structure defined in a shader source code. Currently, we do not handle memory layout automatically. To achieve the correct memory alignment, you might need to pad your data with additional fields. See ",(0,t.jsx)(r.a,{href:"https://www.w3.org/TR/WGSL/#alignment-and-size",children:"WGSL documentation"})," for more details."]})}),"\n"]}),"\n",(0,t.jsxs)(r.li,{children:[(0,t.jsx)(r.code,{children:"resolution"})," - Resolution of a texture where shader will be executed."]}),"\n"]}),"\n",(0,t.jsx)(r.h2,{id:"shaderparam",children:"ShaderParam"}),"\n",(0,t.jsx)(r.pre,{children:(0,t.jsx)(r.code,{className:"language-typescript",children:'type ShaderParam = \n  | { type: "f32"; value: f32 }\n  | { type: "u32"; value: u32 }\n  | { type: "i32"; value: i32 }\n  | { type: "list"; value: ShaderParam[] }\n  | { type: "struct"; value: ShaderParamStructField[] }\n'})}),"\n",(0,t.jsx)(r.h2,{id:"shaderparamstructfield",children:"ShaderParamStructField"}),"\n",(0,t.jsx)(r.pre,{children:(0,t.jsx)(r.code,{className:"language-typescript",children:'type ShaderParamStructField = \n  | { field_name: string; type: "f32"; value: f32 }\n  | { field_name: string; type: "u32"; value: u32 }\n  | { field_name: string; type: "i32"; value: i32 }\n  | { field_name: string; type: "list"; value: ShaderParam[] }\n  | { field_name: string; type: "struct"; value: ShaderParamStructField[] }\n'})})]})}function h(e={}){const{wrapper:r}={...(0,a.a)(),...e.components};return r?(0,t.jsx)(r,{...e,children:(0,t.jsx)(c,{...e})}):c(e)}},1151:(e,r,n)=>{n.d(r,{Z:()=>s,a:()=>d});var t=n(7294);const a={},i=t.createContext(a);function d(e){const r=t.useContext(i);return t.useMemo((function(){return"function"==typeof e?e(r):{...r,...e}}),[r,e])}function s(e){let r;return r=e.disableParentContext?"function"==typeof e.components?e.components(a):e.components||a:d(e.components),t.createElement(i.Provider,{value:r},e.children)}}}]);