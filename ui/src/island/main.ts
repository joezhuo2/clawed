import { mount } from "svelte";
import "../theme.css";
import "./global.css";
import Island from "./Island.svelte";

mount(Island, { target: document.getElementById("app")! });
