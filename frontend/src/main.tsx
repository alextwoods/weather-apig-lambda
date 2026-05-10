import { render } from "preact";
import "./styles/global.css";
import "./styles/layout.css";
import "./styles/panels.css";
import { App } from "./app";

render(<App />, document.getElementById("app")!);
