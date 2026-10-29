-- SPDX-License-Identifier: GPL-3.0-or-later
-- The Ambrotype measuring kit as a Lightroom Classic plug-in
-- (docs/lightroom-trial-plan.md). Add it with File → Plug-in Manager → Add;
-- its commands are under File → Plug-in Extras.
return {
  LrSdkVersion = 6.0,
  LrSdkMinimumVersion = 6.0,
  LrToolkitIdentifier = "org.ambrotype.kit",
  LrPluginName = "Ambrotype measuring kit",
  LrPluginInfoUrl = "https://github.com/shardpelt/Ambrotype/blob/main/docs/lightroom-trial-plan.md",
  LrExportMenuItems = {
    { title = "Run a kit session…", file = "RunSession.lua" },
    { title = "Read the Develop sliders…", file = "ReadSliders.lua" },
  },
  VERSION = { major = 0, minor = 1, revision = 0, build = 0 },
}
