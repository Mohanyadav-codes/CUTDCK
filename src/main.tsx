import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import { Tray } from "./tray/Tray";
import { Studio } from "./studio/Studio";

const isTray = window.location.hash.startsWith("#/tray");
document.body.classList.add(isTray ? "tray" : "studio");

// No browser context menu or reload shortcuts in a desktop app.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest("input, textarea")) e.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isTray ? <Tray /> : <Studio />}</React.StrictMode>,
);
