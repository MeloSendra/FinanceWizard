import { createRoot } from "react-dom/client";
import App from "./App";
import { applyTheme, loadTheme } from "./theme";
import "./styles.css";

// The browser's menu offers Save as, Print and Refresh, which mean nothing here.
// Text fields keep it for paste, and dev builds keep it for Inspect.
if (!import.meta.env.DEV) {
  document.addEventListener("contextmenu", (event) => {
    if (!(event.target instanceof HTMLElement && event.target.closest("input, textarea"))) event.preventDefault();
  });
}

applyTheme(loadTheme());
createRoot(document.getElementById("root") as HTMLElement).render(<App />);
