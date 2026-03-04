import { useState, useEffect } from "react";
import "./App.css";
import { ProjectDashboard } from "./components/ProjectDashboard";
import { TerminalView } from "./components/TerminalView";

function App() {
  const [route, setRoute] = useState(window.location.hash);

  useEffect(() => {
    const onHashChange = () => setRoute(window.location.hash);
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  // Match #/terminal/{projectId}
  const terminalMatch = route.match(/^#\/terminal\/(.+)$/);
  if (terminalMatch) {
    return <TerminalView projectId={terminalMatch[1]} />;
  }

  return (
    <div className="min-h-screen bg-gray-50">
      <header className="border-b border-gray-200 bg-white px-6 py-4">
        <h1 className="text-2xl font-bold text-gray-900">Terrarium</h1>
      </header>
      <main className="mx-auto max-w-5xl px-6 py-8">
        <ProjectDashboard />
      </main>
    </div>
  );
}

export default App;
