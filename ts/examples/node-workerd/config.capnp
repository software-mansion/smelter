using Workerd = import "/workerd/workerd.capnp";

const config :Workerd.Config = (
  services = [
    (name = "main", worker = .mainWorker),
    (name = "internet", network = (allow = ["public", "private", "local"], tlsOptions = (trustBrowserCas = true))),
  ],

  # This is only needed for starting the user code
  sockets = [
    ( name = "http",
      address = "*:8080",
      http = (),
      service = "main"
    ),
  ]
);

const mainWorker :Workerd.Worker = (
  modules = [
    (name = "main", esModule = embed "dist/index.mjs")
  ],
  compatibilityDate = "2026-04-14",
  compatibilityFlags = ["nodejs_compat_v2"],
  globalOutbound = "internet"
);
