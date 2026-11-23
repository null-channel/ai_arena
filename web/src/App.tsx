import { useState, useRef } from 'react';
import { GameControls, type AgentConfig } from './components/GameControls';
import { GameBoard } from './components/GameBoard';
import { GameLog } from './components/GameLog';

function App() {
  const [gameState, setGameState] = useState<any>(null);
  const [gameLogs, setGameLogs] = useState<any[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [activeGameType, setActiveGameType] = useState<string>('');
  const wsRef = useRef<WebSocket | null>(null);

  const handleStartGame = async (gameType: string, agents: AgentConfig[]) => {
    setIsLoading(true);
    setGameLogs([]);
    setGameState(null);
    setActiveGameType(gameType);

    // Close existing WS
    if (wsRef.current) {
      wsRef.current.close();
    }

    try {
      const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';

      // 1. Create Game
      const response = await fetch('/api/games', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          game_type: gameType,
          agents: agents.map(a => ({
            agent: a.kind,
            model: a.model,
            temp: a.temperature,
            seed: a.seed ?? null,
            secret_profile: a.secret_profile ?? null,
            api_endpoint: a.api_endpoint ?? null,
          }))
        })
      });

      if (!response.ok) throw new Error('Failed to create game');

      const { game_id } = await response.json();

      // 2. Connect WebSocket
      const ws = new WebSocket(`${wsProtocol}//${window.location.host}/ws/games/${game_id}`);
      wsRef.current = ws;

      ws.onopen = () => {
        console.log('Connected to game server');
        setIsLoading(false);
      };

      ws.onmessage = (event) => {
        const data = JSON.parse(event.data);

        if (data.type === 'complete') {
          console.log('Game complete');
          return;
        }

        if (data.error) {
          console.error('Game error:', data.error);
          setGameLogs(prev => [...prev, { error: data.error }]);
          return;
        }

        // Update state and logs
        setGameState(data);
        setGameLogs(prev => [...prev, data]);
      };

      ws.onerror = (error) => {
        console.error('WebSocket error:', error);
        setIsLoading(false);
      };

      ws.onclose = () => {
        console.log('Disconnected from game server');
      };

    } catch (error) {
      console.error('Error starting game:', error);
      setIsLoading(false);
      alert('Failed to start game. Check console for details.');
    }
  };

  return (
    <div className="min-h-screen bg-gray-900 text-gray-100 font-sans selection:bg-blue-500/30">
      <div className="max-w-7xl mx-auto p-6">
        <header className="mb-8 flex items-center justify-between">
          <div className="flex items-center space-x-4">
            <div className="bg-gradient-to-br from-blue-600 to-purple-600 p-3 rounded-xl shadow-lg shadow-blue-500/20">
              <svg className="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
              </svg>
            </div>
            <div>
              <h1 className="text-3xl font-black tracking-tight text-white">
                AI <span className="text-transparent bg-clip-text bg-gradient-to-r from-blue-400 to-purple-400">ARENA</span>
              </h1>
              <p className="text-gray-400 text-sm font-medium">Autonomous Agent Battleground</p>
            </div>
          </div>
          <div className="flex items-center space-x-2 text-sm text-gray-500 bg-gray-800 px-4 py-2 rounded-full border border-gray-700">
            <div className="w-2 h-2 rounded-full bg-green-500 animate-pulse"></div>
            <span>System Online</span>
          </div>
        </header>

        <div className="grid grid-cols-1 lg:grid-cols-12 gap-8">
          {/* Left Column: Controls */}
          <div className="lg:col-span-4 space-y-6">
            <GameControls onStartGame={handleStartGame} isLoading={isLoading} />

            <div className="bg-gray-800 p-6 rounded-xl border border-gray-700 text-sm text-gray-400">
              <h3 className="font-bold text-white mb-2">Instructions</h3>
              <ul className="list-disc list-inside space-y-1">
                <li>Select a game mode</li>
                <li>Configure AI agents (Model & Provider)</li>
                <li>Click "Start Battle" to begin</li>
                <li>Watch the agents compete in real-time!</li>
              </ul>
            </div>
          </div>

          {/* Right Column: Game Board & Logs */}
          <div className="lg:col-span-8 space-y-6">
            <GameBoard gameState={gameState} gameType={activeGameType} />
            <GameLog logs={gameLogs} />
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
