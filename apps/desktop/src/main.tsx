import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { ErrorBoundary } from "./app/ErrorBoundary";
import { installGlobalErrorHandlers } from "./app/errors";
// Bundled locally (OFL-1.1): the app never fetches fonts from the network.
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";

installGlobalErrorHandlers();

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");
createRoot(root).render(
  <StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </StrictMode>,
);
