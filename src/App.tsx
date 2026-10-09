import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [ringing, setRinging] = useState(false);
  const activeRef = useRef(false);

  const startBell = useCallback(() => {
    if (activeRef.current) return;
    activeRef.current = true;
    setRinging(true);
    invoke("start_bell").catch(console.error);
  }, []);

  const stopBell = useCallback(() => {
    if (!activeRef.current) return;
    activeRef.current = false;
    setRinging(false);
    invoke("stop_bell").catch(console.error);
  }, []);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.code === "Space") {
        e.preventDefault();
        startBell();
      }
    };
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.code === "Space") {
        e.preventDefault();
        stopBell();
      }
    };
    // Avoid a bell stuck ringing if the window loses focus mid-press.
    const onBlur = () => stopBell();

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", onBlur);
    };
  }, [startBell, stopBell]);

  return (
    <main className="container">
      <button
        type="button"
        className={`bell-button ${ringing ? "ringing" : ""}`}
        onPointerDown={(e) => {
          e.preventDefault();
          startBell();
        }}
        onPointerUp={stopBell}
        onPointerLeave={stopBell}
        onPointerCancel={stopBell}
        aria-pressed={ringing}
      >
        <span className="bell-icon">🔔</span>
      </button>
      <p className="hint">
        Klik dan tahan tombol, atau tekan &amp; tahan <kbd>Spasi</kbd>.
        <br />
        Lama bunyi bel mengikuti lama tombol ditekan.
      </p>
    </main>
  );
}

export default App;
