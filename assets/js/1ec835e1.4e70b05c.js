"use strict";(self.webpackChunkcompositor_live=self.webpackChunkcompositor_live||[]).push([[249],{4742:(n,e,t)=>{t.r(e),t.d(e,{assets:()=>_,contentTitle:()=>p,default:()=>m,frontMatter:()=>u,metadata:()=>h,toc:()=>f});var i=t(5893),r=t(1151),s=t(4866),a=t(5162);const o=t.p+"assets/images/view_transition_1-28a1c86c833294d036082d4b4bc7af46.webp",d=t.p+"assets/images/view_transition_2-c1afc516562434973e3e847009ccf801.webp",l=t.p+"assets/images/view_transition_3-c3bf0e98e0ac53e801aefe065717c771.webp",c=t.p+"assets/images/view_transition_4-6d54cf14f1afdc6654a50c29572a02d0.webp",u={},p="Transitions (View/Rescaler)",h={id:"guides/view-transition",title:"Transitions (View/Rescaler)",description:"This guide will show a few basic examples of animated transitions on View/Rescaler components.",source:"@site/pages/guides/view-transition.md",sourceDirName:"guides",slug:"/guides/view-transition",permalink:"/docs/guides/view-transition",draft:!1,unlisted:!1,tags:[],version:"current",frontMatter:{},sidebar:"sidebar",previous:{title:"Basic Layouts",permalink:"/docs/guides/basic-layouts"},next:{title:"Concepts",permalink:"/docs/concept/overview"}},_={},f=[{value:"Configure inputs and output",id:"configure-inputs-and-output",level:3},{value:"Transition that changes the <code>width</code> of an input stream",id:"transition-that-changes-the-width-of-an-input-stream",level:3},{value:"Transition on one of the sibling components",id:"transition-on-one-of-the-sibling-components",level:3},{value:"Transition between different modes",id:"transition-between-different-modes",level:3},{value:"Different interpolation functions",id:"different-interpolation-functions",level:3}];function g(n){const e={a:"a",code:"code",h1:"h1",h3:"h3",li:"li",p:"p",pre:"pre",ul:"ul",...(0,r.a)(),...n.components};return(0,i.jsxs)(i.Fragment,{children:[(0,i.jsx)(e.h1,{id:"transitions-viewrescaler",children:"Transitions (View/Rescaler)"}),"\n",(0,i.jsxs)(e.p,{children:["This guide will show a few basic examples of animated transitions on ",(0,i.jsx)(e.code,{children:"View"}),"/",(0,i.jsx)(e.code,{children:"Rescaler"})," components."]}),"\n",(0,i.jsx)(e.h3,{id:"configure-inputs-and-output",children:"Configure inputs and output"}),"\n",(0,i.jsxs)(e.p,{children:['Start the compositor and configure 2 input streams and a single output stream as described in the "Simple scene"\nguide in the ',(0,i.jsx)(e.a,{href:"/docs/guides/quick-start#configure-inputs-and-output",children:'"Configure inputs and output"'})," section."]}),"\n",(0,i.jsxs)(e.h3,{id:"transition-that-changes-the-width-of-an-input-stream",children:["Transition that changes the ",(0,i.jsx)(e.code,{children:"width"})," of an input stream"]}),"\n",(0,i.jsxs)(s.Z,{queryString:"lang",children:[(0,i.jsxs)(a.Z,{value:"http",label:"HTTP",children:[(0,i.jsx)(e.p,{children:"Set initial scene for the transition:"}),(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-http",children:'POST: /api/output/output_1/update\nContent-Type: application/json\n\n{\n  "video": {\n    "root": {\n      "type": "view",\n      "background_color_rgba": "#4d4d4dff",\n      "children": [\n        {\n          "type": "rescaler",\n          "id": "rescaler_1",\n          // highlight-start\n          "width": 480,\n          // highlight-end\n          "child": { "type": "input_stream", "input_id": "input_1" },\n        }\n      ]\n    }\n  }\n}\n'})}),(0,i.jsxs)(e.p,{children:["A few seconds latter update a scene with a different ",(0,i.jsx)(e.code,{children:"width"}),":"]}),(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-http",children:'POST: /api/output/output_1/update\nContent-Type: application/json\n\n{\n  "video": {\n    "root": {\n      "type": "view",\n      "background_color_rgba": "#4d4d4dff",\n      "children": [\n        {\n          "type": "rescaler",\n          "id": "rescaler_1",\n          // highlight-start\n          "width": 1280,\n          "transition": { "duration_ms": 2000 },\n          // highlight-end\n          "child": { "type": "input_stream", "input_id": "input_1" },\n        }\n      ]\n    }\n  }\n}\n'})})]}),(0,i.jsxs)(a.Z,{value:"membrane",label:"Membrane Framework",children:[(0,i.jsxs)(e.p,{children:["Set initial scene for the transition and after few seconds update a component\nwith a different ",(0,i.jsx)(e.code,{children:"width"}),":"]}),(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-elixir",children:'def handle_setup(ctx, state) do\n  request = %LiveCompositor.Request.UpdateVideoOutput{\n    output_id: "output_1",\n    root: %{\n      type: :view,\n      background_color_rgba: "#4d4d4dff",\n      children: [\n        %{\n          type: :rescaler,\n          id: "rescaler_1",\n          // highlight-start\n          width: 480,\n          // highlight-end\n          child: %{ type: :input_stream, input_id: :input_1 },\n        }\n      ]\n    }\n  }\n  Process.send_after(self(), :start_transition, 2000)\n  {[notify_child: {:live_compositor, request}], state}\nend\n\ndef handle_info(:start_transition, _ctx, state)\n  request = %LiveCompositor.Request.UpdateVideoOutput{\n    output_id: "output_1",\n    root: %{\n      type: :view,\n      background_color_rgba: "#4d4d4dff",\n      children: [\n        %{\n          type: :rescaler,\n          id: "rescaler_1",\n          // highlight-start\n          width: 1280,\n          transition: %{ duration_ms: 2000 },\n          // highlight-end\n          child: %{ type: :input_stream, input_id: :input_1 },\n        }\n      ]\n    }\n  }\n  {[notify_child: {:live_compositor, request}], state}\nend\n'})})]})]}),"\n",(0,i.jsxs)(e.p,{children:["In the first update request, you can see that the rescaler has a width of 480, and in the second one, it is changed\nto 1280 and ",(0,i.jsx)(e.code,{children:"transition.duration_ms: 2000"})," was added."]}),"\n",(0,i.jsxs)(e.p,{children:["The component must have the same ",(0,i.jsx)(e.code,{children:'"id"'})," in both the initial state and the update that starts the\ntransition, otherwise it will switch immediately to the new state without a transition."]}),"\n",(0,i.jsxs)("div",{style:{textAlign:"center"},children:[(0,i.jsx)("img",{src:o,style:{width:600}}),(0,i.jsx)(e.p,{children:"Output stream"})]}),"\n",(0,i.jsx)(e.h3,{id:"transition-on-one-of-the-sibling-components",children:"Transition on one of the sibling components"}),"\n",(0,i.jsx)(e.p,{children:"In the above scenario you saw how transition on a single component behaves, but let's see what happens with\ncomponents that are not a part of the transition, but their size and position still depend on other components."}),"\n",(0,i.jsxs)(e.p,{children:["Add a second input stream wrapped with ",(0,i.jsx)(e.code,{children:"Rescaler"}),", but without any transition options."]}),"\n",(0,i.jsxs)(s.Z,{queryString:"lang",children:[(0,i.jsxs)(a.Z,{value:"http",label:"HTTP",children:[(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-http",children:'POST: /api/output/output_1/update\nContent-Type: application/json\n\n{\n  "video": {\n    "root": {\n      "type": "view",\n      "background_color_rgba": "#4d4d4dff",\n      "children": [\n        {\n          "type": "rescaler",\n          "id": "rescaler_1",\n          // highlight-start\n          "width": 480,\n          // highlight-end\n          "child": { "type": "input_stream", "input_id": "input_1" },\n        },\n        {\n          "type": "rescaler",\n          "child": { "type": "input_stream", "input_id": "input_2" },\n        }\n      ]\n    }\n  }\n}\n'})}),(0,i.jsxs)(e.p,{children:["Update a scene with a different ",(0,i.jsx)(e.code,{children:"width"}),":"]}),(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-http",children:'POST: /api/output/output_1/update\nContent-Type: application/json\n\n{\n  "video": {\n    "root": {\n      "type": "view",\n      "background_color_rgba": "#4d4d4dff",\n      "children": [\n        {\n          "type": "rescaler",\n          "id": "rescaler_1",\n          // highlight-start\n          "width": 1280,\n          "transition": { "duration_ms": 2000 },\n          // highlight-end\n          "child": { "type": "input_stream", "input_id": "input_1" },\n        },\n        {\n          "type": "rescaler",\n          "child": { "type": "input_stream", "input_id": "input_2" },\n        }\n      ]\n    }\n  }\n}\n'})})]}),(0,i.jsx)(a.Z,{value:"membrane",label:"Membrane Framework",children:(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-elixir",children:'def handle_setup(ctx, state) do\n  request = %LiveCompositor.Request.UpdateVideoOutput{\n    output_id: "output_1",\n    root: %{\n      type: :view,\n      background_color_rgba: "#4d4d4dff",\n      children: [\n        %{\n          type: :rescaler,\n          id: "rescaler_1",\n          // highlight-start\n          width: 480,\n          // highlight-end\n          child: %{ type: :input_stream, input_id: :input_1 },\n        },\n        %{\n          type: :rescaler,\n          child: %{ type: :input_stream, input_id: :input_2 },\n        }\n      ]\n    }\n  }\n  Process.send_after(self(), :start_transition, 2000)\n  {[notify_child: {:live_compositor, request}], state}\nend\n\ndef handle_info(:start_transition, _ctx, state)\n  request = %LiveCompositor.Request.UpdateVideoOutput{\n    output_id: "output_1",\n    root: %{\n      type: :view,\n      background_color_rgba: "#4d4d4dff",\n      children: [\n        %{\n          type: :rescaler,\n          id: "rescaler_1",\n          // highlight-start\n          width: 1280,\n          transition: %{ duration_ms: 2000 },\n          // highlight-end\n          child: %{ type: :input_stream, input_id: :input_1 },\n        },\n        %{\n          type: :rescaler,\n          child: %{ type: :input_stream, input_id: :input_2 },\n        }\n      ]\n    }\n  }\n  {[notify_child: {:live_compositor, request}], state}\nend\n'})})})]}),"\n",(0,i.jsxs)("div",{style:{textAlign:"center"},children:[(0,i.jsx)("img",{src:d,style:{width:600}}),(0,i.jsx)(e.p,{children:"Output stream"})]}),"\n",(0,i.jsx)(e.h3,{id:"transition-between-different-modes",children:"Transition between different modes"}),"\n",(0,i.jsx)(e.p,{children:"Currently, a state before the transition and after needs to use the same type of configuration. In particular:"}),"\n",(0,i.jsxs)(e.ul,{children:["\n",(0,i.jsx)(e.li,{children:"It is not possible to transition a component between static and absolute positioning."}),"\n",(0,i.jsxs)(e.li,{children:["It is not possible to transition a component between using ",(0,i.jsx)(e.code,{children:"top"})," and ",(0,i.jsx)(e.code,{children:"bottom"})," fields (the same for ",(0,i.jsx)(e.code,{children:"left"}),"/",(0,i.jsx)(e.code,{children:"right"}),")."]}),"\n",(0,i.jsxs)(e.li,{children:["It is not possible to transition a component with known ",(0,i.jsx)(e.code,{children:"width"}),"/",(0,i.jsx)(e.code,{children:"height"})," to a state with dynamic ",(0,i.jsx)(e.code,{children:"width"}),"/",(0,i.jsx)(e.code,{children:"height"})," based\non the parent layout."]}),"\n"]}),"\n",(0,i.jsxs)(e.p,{children:["Let's try the same example as in the first scenario with a single input, but instead, change the ",(0,i.jsx)(e.code,{children:"Rescaler"})," component to be absolutely positioned in the second update."]}),"\n",(0,i.jsxs)(s.Z,{queryString:"lang",children:[(0,i.jsxs)(a.Z,{value:"http",label:"HTTP",children:[(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-http",children:'POST: /api/output/output_1/update\nContent-Type: application/json\n\n{\n  "video": {\n    "root": {\n      "type": "view",\n      "background_color_rgba": "#4d4d4dff",\n      "children": [\n        {\n          "type": "rescaler",\n          "id": "rescaler_1",\n          // highlight-start\n          "width": 480,\n          // highlight-end\n          "child": { "type": "input_stream", "input_id": "input_1" },\n        }\n      ]\n    }\n  }\n}\n'})}),(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-http",children:'POST: /api/output/output_1/update\nContent-Type: application/json\n\n{\n  "video": {\n    "root": {\n      "type": "view",\n      "background_color_rgba": "#4d4d4dff",\n      "children": [\n        {\n          "type": "rescaler",\n          "id": "rescaler_1",\n          // highlight-start\n          "width": 1280,\n          "top": 0,\n          "left": 0,\n          "transition": { "duration_ms": 2000 },\n          // highlight-end\n          "child": { "type": "input_stream", "input_id": "input_1" },\n        }\n      ]\n    }\n  }\n}\n'})})]}),(0,i.jsx)(a.Z,{value:"membrane",label:"Membrane Framework",children:(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-elixir",children:'def handle_setup(ctx, state) do\n  request = %LiveCompositor.Request.UpdateVideoOutput{\n    output_id: "output_1",\n    root: %{\n      type: :view,\n      background_color_rgba: "#4d4d4dff",\n      children: [\n        %{\n          type: :rescaler,\n          id: "rescaler_1",\n          // highlight-start\n          width: 480,\n          // highlight-end\n          child: %{ type: :input_stream, input_id: :input_1 },\n        }\n      ]\n    }\n  }\n  Process.send_after(self(), :start_transition, 2000)\n  {[notify_child: {:live_compositor, request}], state}\nend\n\ndef handle_info(:start_transition, _ctx, state)\n  request = %LiveCompositor.Request.UpdateVideoOutput{\n    output_id: "output_1",\n    root: %{\n      type: :view,\n      background_color_rgba: "#4d4d4dff",\n      children: [\n        %{\n          type: :rescaler,\n          id: "rescaler_1",\n          // highlight-start\n          width: 1280,\n          top: 0,\n          left: 0,\n          transition: %{ duration_ms: 2000 },\n          // highlight-end\n          child: %{ type: :input_stream, input_id: :input_1 },\n        }\n      ]\n    }\n  }\n  {[notify_child: {:live_compositor, request}], state}\nend\n'})})})]}),"\n",(0,i.jsxs)(e.p,{children:["As you can see on the resulting stream, the transition did not happen because the ",(0,i.jsx)(e.code,{children:"Rescaler"})," component\nin the initial scene was using static positioning and after the update it was positioned absolutely."]}),"\n",(0,i.jsxs)("div",{style:{textAlign:"center"},children:[(0,i.jsx)("img",{src:l,style:{width:600}}),(0,i.jsx)(e.p,{children:"Output stream"})]}),"\n",(0,i.jsx)(e.h3,{id:"different-interpolation-functions",children:"Different interpolation functions"}),"\n",(0,i.jsx)(e.p,{children:"All of the above examples use default linear interpolation, but there are also a few other\nmodes available."}),"\n",(0,i.jsxs)(s.Z,{queryString:"lang",children:[(0,i.jsxs)(a.Z,{value:"http",label:"HTTP",children:[(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-http",children:'POST: /api/output/output_1/update\nContent-Type: application/json\n\n{\n  "video": {\n    "root": {\n      "type": "view",\n      "background_color_rgba": "#4d4d4dff",\n      "children": [\n        {\n          "type": "rescaler",\n          "id": "rescaler_1",\n          "width": 320, "height": 180, "top": 0, "left": 0,\n          "child": { "type": "input_stream", "input_id": "input_1" },\n        },\n        {\n          "type": "rescaler",\n          "id": "rescaler_2",\n          "width": 320, "height": 180, "top": 0, "left": 320,\n          "child": { "type": "input_stream", "input_id": "input_2" },\n        },\n        {\n          "type": "rescaler",\n          "id": "rescaler_3",\n          "width": 320, "height": 180, "top": 0, "left": 640,\n          "child": { "type": "input_stream", "input_id": "input_3" },\n        },\n        {\n          "type": "rescaler",\n          "id": "rescaler_4",\n          "width": 320, "height": 180, "top": 0, "left": 960,\n          "child": { "type": "input_stream", "input_id": "input_4" },\n        },\n      ]\n    }\n  }\n}\n'})}),(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-http",children:'POST: /api/output/output_1/update\nContent-Type: application/json\n\n{\n  "video": {\n    "root": {\n      "type": "view",\n      "background_color_rgba": "#4d4d4dff",\n      "children": [\n        {\n          "type": "rescaler",\n          "id": "rescaler_1",\n          "width": 320, "height": 180, "top": 540, "left": 0,\n          "child": { "type": "input_stream", "input_id": "input_1" },\n          "transition": { "duration_ms": 2000 },\n        },\n        {\n          "type": "rescaler",\n          "id": "rescaler_2",\n          "width": 320, "height": 180, "top": 540, "left": 320,\n          "child": { "type": "input_stream", "input_id": "input_2" },\n          "transition": {\n            "duration_ms": 2000, "easing_function": {"function_name": "bounce"}\n          },\n        },\n        {\n          "type": "rescaler",\n          "id": "rescaler_3",\n          "width": 320, "height": 180, "top": 540, "left": 640,\n          "child": { "type": "input_stream", "input_id": "input_3" },\n          "transition": {\n            "duration_ms": 2000,\n            "easing_function": {\n                "function_name": "cubic_bezier",\n                "points": [0.65, 0, 0.35, 1]\n            }\n          }\n        },\n        {\n          "type": "rescaler",\n          "id": "rescaler_4",\n          "width": 320, "height": 180, "top": 540, "left": 960,\n          "child": { "type": "input_stream", "input_id": "input_4" },\n          "transition": {\n            "duration_ms": 2000,\n            "easing_function": {\n              "function_name": "cubic_bezier",\n              "points": [0.33, 1, 0.68, 1]\n            }\n          }\n        }\n      ]\n    }\n  }\n}\n'})})]}),(0,i.jsx)(a.Z,{value:"membrane",label:"Membrane Framework",children:(0,i.jsx)(e.pre,{children:(0,i.jsx)(e.code,{className:"language-elixir",children:'def handle_setup(ctx, state) do\n  request = %LiveCompositor.Request.UpdateVideoOutput{\n    output_id: "output_1",\n    root: %{\n      type: :view,\n      background_color_rgba: "#4d4d4dff",\n      children: [\n        %{\n          type: :rescaler,\n          id: "rescaler_1",\n          width: 320, height: 180, top: 0, left: 0,\n          child: %{ type: :input_stream, input_id: :input_1 },\n        },\n        %{\n          type: :rescaler,\n          id: "rescaler_2",\n          width: 320, height: 180, top: 0, left: 320,\n          child: %{ type: :input_stream, input_id: :input_2 },\n        },\n        %{\n          type: :rescaler,\n          id: "rescaler_3",\n          width: 320, height: 180, top: 0, left: 640,\n          child: %{ type: :input_stream, input_id: :input_3 },\n        },\n        %{\n          type: :rescaler,\n          id: "rescaler_4",\n          width: 320, height: 180, top: 0, left: 960,\n          child: %{ type: :input_stream, input_id: :input_4 },\n        }\n      ]\n    }\n  }\n  Process.send_after(self(), :start_transition, 2000)\n  {[notify_child: {:live_compositor, request}], state}\nend\n\ndef handle_info(:start_transition, _ctx, state)\n  request = %LiveCompositor.Request.UpdateVideoOutput{\n    output_id: "output_1",\n    root: %{\n      type: :view,\n      background_color_rgba: "#4d4d4dff",\n      children: [\n        %{\n          type: :rescaler,\n          id: "rescaler_1",\n          width: 320, height: 180, top: 0, left: 0,\n          child: %{ type: :input_stream, input_id: :input_1 },\n          transition: %{ duration_ms: 2000 },\n        },\n        %{\n          type: :rescaler,\n          id: "rescaler_2",\n          width: 320, height: 180, top: 0, left: 320,\n          child: %{ type: :input_stream, input_id: :input_2 },\n          transition: %{\n            duration_ms: 2000\n            easing_function: %{ function_name: :bounce}\n          },\n        },\n        %{\n          type: :rescaler,\n          id: "rescaler_3",\n          width: 320, height: 180, top: 0, left: 640,\n          child: %{ type: :input_stream, input_id: :input_3 },\n          transition: %{\n            duration_ms: 2000\n            easing_function: %{\n              function_name: :cubic_bezier,\n              points: [0.65, 0, 0.35, 1]\n            }\n          }\n        },\n        %{\n          type: :rescaler,\n          id: "rescaler_4",\n          width: 320, height: 180, top: 0, left: 960,\n          child: %{ type: :input_stream, input_id: :input_4 },\n          transition: %{\n            duration_ms: 2000\n            easing_function: %{\n              function_name: :cubic_bezier,\n              points: [0.33, 1, 0.68, 1]\n            }\n          }\n        }\n      ]\n    }\n  }\n  {[notify_child: {:live_compositor, request}], state}\nend\n'})})})]}),"\n",(0,i.jsxs)("div",{style:{textAlign:"center"},children:[(0,i.jsx)("img",{src:c,style:{width:600}}),(0,i.jsx)(e.p,{children:"Output stream"})]}),"\n",(0,i.jsxs)(e.ul,{children:["\n",(0,i.jsxs)(e.li,{children:[(0,i.jsx)(e.code,{children:"Input 1"})," - Linear transition"]}),"\n",(0,i.jsxs)(e.li,{children:[(0,i.jsx)(e.code,{children:"Input 2"})," - Bounce transition"]}),"\n",(0,i.jsxs)(e.li,{children:[(0,i.jsx)(e.code,{children:"Input 3"})," - Cubic B\xe9zier transition with ",(0,i.jsx)(e.code,{children:"[0.65, 0, 0.35, 1]"})," points (",(0,i.jsx)(e.a,{href:"https://easings.net/#easeInOutCubic",children:(0,i.jsx)(e.code,{children:"easeInOutCubic"})}),")"]}),"\n",(0,i.jsxs)(e.li,{children:[(0,i.jsx)(e.code,{children:"Input 4"})," - Cubic B\xe9zier transition with ",(0,i.jsx)(e.code,{children:"[0.33, 1, 0.68, 1]"})," points (",(0,i.jsx)(e.a,{href:"https://easings.net/#easeOutCubic",children:(0,i.jsx)(e.code,{children:"easeOutCubic"})}),")"]}),"\n"]}),"\n",(0,i.jsxs)(e.p,{children:["Check out other popular Cubic B\xe9zier curves on ",(0,i.jsx)(e.a,{href:"https://easings.net",children:"https://easings.net"}),"."]})]})}function m(n={}){const{wrapper:e}={...(0,r.a)(),...n.components};return e?(0,i.jsx)(e,{...n,children:(0,i.jsx)(g,{...n})}):g(n)}},5162:(n,e,t)=>{t.d(e,{Z:()=>a});t(7294);var i=t(6905);const r={tabItem:"tabItem_Ymn6"};var s=t(5893);function a(n){let{children:e,hidden:t,className:a}=n;return(0,s.jsx)("div",{role:"tabpanel",className:(0,i.Z)(r.tabItem,a),hidden:t,children:e})}},4866:(n,e,t)=>{t.d(e,{Z:()=>j});var i=t(7294),r=t(6905),s=t(2466),a=t(6550),o=t(469),d=t(1980),l=t(7392),c=t(12);function u(n){return i.Children.toArray(n).filter((n=>"\n"!==n)).map((n=>{if(!n||(0,i.isValidElement)(n)&&function(n){const{props:e}=n;return!!e&&"object"==typeof e&&"value"in e}(n))return n;throw new Error(`Docusaurus error: Bad <Tabs> child <${"string"==typeof n.type?n.type:n.type.name}>: all children of the <Tabs> component should be <TabItem>, and every <TabItem> should have a unique "value" prop.`)}))?.filter(Boolean)??[]}function p(n){const{values:e,children:t}=n;return(0,i.useMemo)((()=>{const n=e??function(n){return u(n).map((n=>{let{props:{value:e,label:t,attributes:i,default:r}}=n;return{value:e,label:t,attributes:i,default:r}}))}(t);return function(n){const e=(0,l.l)(n,((n,e)=>n.value===e.value));if(e.length>0)throw new Error(`Docusaurus error: Duplicate values "${e.map((n=>n.value)).join(", ")}" found in <Tabs>. Every value needs to be unique.`)}(n),n}),[e,t])}function h(n){let{value:e,tabValues:t}=n;return t.some((n=>n.value===e))}function _(n){let{queryString:e=!1,groupId:t}=n;const r=(0,a.k6)(),s=function(n){let{queryString:e=!1,groupId:t}=n;if("string"==typeof e)return e;if(!1===e)return null;if(!0===e&&!t)throw new Error('Docusaurus error: The <Tabs> component groupId prop is required if queryString=true, because this value is used as the search param name. You can also provide an explicit value such as queryString="my-search-param".');return t??null}({queryString:e,groupId:t});return[(0,d._X)(s),(0,i.useCallback)((n=>{if(!s)return;const e=new URLSearchParams(r.location.search);e.set(s,n),r.replace({...r.location,search:e.toString()})}),[s,r])]}function f(n){const{defaultValue:e,queryString:t=!1,groupId:r}=n,s=p(n),[a,d]=(0,i.useState)((()=>function(n){let{defaultValue:e,tabValues:t}=n;if(0===t.length)throw new Error("Docusaurus error: the <Tabs> component requires at least one <TabItem> children component");if(e){if(!h({value:e,tabValues:t}))throw new Error(`Docusaurus error: The <Tabs> has a defaultValue "${e}" but none of its children has the corresponding value. Available values are: ${t.map((n=>n.value)).join(", ")}. If you intend to show no default tab, use defaultValue={null} instead.`);return e}const i=t.find((n=>n.default))??t[0];if(!i)throw new Error("Unexpected error: 0 tabValues");return i.value}({defaultValue:e,tabValues:s}))),[l,u]=_({queryString:t,groupId:r}),[f,g]=function(n){let{groupId:e}=n;const t=function(n){return n?`docusaurus.tab.${n}`:null}(e),[r,s]=(0,c.Nk)(t);return[r,(0,i.useCallback)((n=>{t&&s.set(n)}),[t,s])]}({groupId:r}),m=(()=>{const n=l??f;return h({value:n,tabValues:s})?n:null})();(0,o.Z)((()=>{m&&d(m)}),[m]);return{selectedValue:a,selectValue:(0,i.useCallback)((n=>{if(!h({value:n,tabValues:s}))throw new Error(`Can't select invalid tab value=${n}`);d(n),u(n),g(n)}),[u,g,s]),tabValues:s}}var g=t(2389);const m={tabList:"tabList__CuJ",tabItem:"tabItem_LNqP"};var b=t(5893);function x(n){let{className:e,block:t,selectedValue:i,selectValue:a,tabValues:o}=n;const d=[],{blockElementScrollPositionUntilNextRender:l}=(0,s.o5)(),c=n=>{const e=n.currentTarget,t=d.indexOf(e),r=o[t].value;r!==i&&(l(e),a(r))},u=n=>{let e=null;switch(n.key){case"Enter":c(n);break;case"ArrowRight":{const t=d.indexOf(n.currentTarget)+1;e=d[t]??d[0];break}case"ArrowLeft":{const t=d.indexOf(n.currentTarget)-1;e=d[t]??d[d.length-1];break}}e?.focus()};return(0,b.jsx)("ul",{role:"tablist","aria-orientation":"horizontal",className:(0,r.Z)("tabs",{"tabs--block":t},e),children:o.map((n=>{let{value:e,label:t,attributes:s}=n;return(0,b.jsx)("li",{role:"tab",tabIndex:i===e?0:-1,"aria-selected":i===e,ref:n=>d.push(n),onKeyDown:u,onClick:c,...s,className:(0,r.Z)("tabs__item",m.tabItem,s?.className,{"tabs__item--active":i===e}),children:t??e},e)}))})}function y(n){let{lazy:e,children:t,selectedValue:r}=n;const s=(Array.isArray(t)?t:[t]).filter(Boolean);if(e){const n=s.find((n=>n.props.value===r));return n?(0,i.cloneElement)(n,{className:"margin-top--md"}):null}return(0,b.jsx)("div",{className:"margin-top--md",children:s.map(((n,e)=>(0,i.cloneElement)(n,{key:e,hidden:n.props.value!==r})))})}function w(n){const e=f(n);return(0,b.jsxs)("div",{className:(0,r.Z)("tabs-container",m.tabList),children:[(0,b.jsx)(x,{...n,...e}),(0,b.jsx)(y,{...n,...e})]})}function j(n){const e=(0,g.Z)();return(0,b.jsx)(w,{...n,children:u(n.children)},String(e))}},1151:(n,e,t)=>{t.d(e,{Z:()=>o,a:()=>a});var i=t(7294);const r={},s=i.createContext(r);function a(n){const e=i.useContext(s);return i.useMemo((function(){return"function"==typeof n?n(e):{...e,...n}}),[e,n])}function o(n){let e;return e=n.disableParentContext?"function"==typeof n.components?n.components(r):n.components||r:a(n.components),i.createElement(s.Provider,{value:e},n.children)}}}]);