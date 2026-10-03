import { getCurrentWindow } from "@tauri-apps/api/window";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { Spotlight } from "./components/Spotlight";
import "./styles/theme.css";

// One bundle, two windows: the main app and the universal search bar.
const isSpotlight = getCurrentWindow().label === "spotlight";
if (isSpotlight) document.documentElement.classList.add("spot-window");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isSpotlight ? <Spotlight /> : <App />}</React.StrictMode>,
);
