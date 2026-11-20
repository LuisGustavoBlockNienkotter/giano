import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { api } from "./lib/api";

// Syntax-highlighting CSS comes from the Rust renderer (syntect), light and dark.
api.highlightCss().then(({ light, dark }) => {
  const style = document.createElement("style");
  style.textContent = `${light}\n@media (prefers-color-scheme: dark) {\n${dark}\n}`;
  document.head.appendChild(style);
});

export default mount(App, { target: document.getElementById("app")! });
