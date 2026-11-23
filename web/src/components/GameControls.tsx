import React, { useState, useEffect } from 'react';

export interface AgentConfig {
    name: string;
    kind: 'OpenAI' | 'Anthropic' | 'Ollama' | 'Gemini';
    model: string;
    temperature: number;
    seed?: number;
    secret_profile?: string;
    api_endpoint?: string;
}

interface GameControlsProps {
    onStartGame: (gameType: string, agents: AgentConfig[]) => void;
    isLoading: boolean;
}

const AVAILABLE_GAMES = [
    { id: 'TicTacToe', name: 'Tic-Tac-Toe' },
    { id: 'ConnectFour', name: 'Connect Four' },
    { id: 'RockPaperScissors', name: 'Rock Paper Scissors' },
    { id: 'Chess', name: 'Chess' },
];

const AGENT_KINDS = ['OpenAI', 'Anthropic', 'Ollama', 'Gemini'];

export const GameControls: React.FC<GameControlsProps> = ({ onStartGame, isLoading }) => {
    const [gameType, setGameType] = useState(AVAILABLE_GAMES[0].id);
    const [availableSecrets, setAvailableSecrets] = useState<string[]>([]);

    useEffect(() => {
        fetch('/api/secrets')
            .then(res => res.json())
            .then(data => {
                console.log('Fetched secrets:', data.profiles);
                setAvailableSecrets(data.profiles);
            })
            .catch(err => console.error('Failed to fetch secrets:', err));
    }, []);

    // Auto-select first available secret profile if current one is invalid
    useEffect(() => {
        if (availableSecrets.length > 0) {
            if (!agent1.secret_profile || !availableSecrets.includes(agent1.secret_profile)) {
                setAgent1(prev => ({ ...prev, secret_profile: availableSecrets[0] }));
            }
            if (!agent2.secret_profile || !availableSecrets.includes(agent2.secret_profile)) {
                setAgent2(prev => ({ ...prev, secret_profile: availableSecrets[0] }));
            }
        }
    }, [availableSecrets]);

    const [agent1, setAgent1] = useState<AgentConfig>({
        name: 'Agent 1',
        kind: 'OpenAI',
        model: 'gpt-3.5-turbo',
        temperature: 0.7,
        secret_profile: 'openai.default',
    });

    const [agent2, setAgent2] = useState<AgentConfig>({
        name: 'Agent 2',
        kind: 'OpenAI',
        model: 'gpt-3.5-turbo',
        temperature: 0.7,
        secret_profile: 'openai.default',
    });

    const handleStart = () => {
        onStartGame(gameType, [agent1, agent2]);
    };

    const updateAgent = (
        index: 1 | 2,
        field: keyof AgentConfig,
        value: string | number
    ) => {
        const setter = index === 1 ? setAgent1 : setAgent2;
        setter((prev) => ({ ...prev, [field]: value }));
    };

    const renderAgentConfig = (agent: AgentConfig, index: 1 | 2, color: string) => (
        <div className={`bg-gray-700/50 p-4 rounded-lg border border-gray-600`}>
            <h3 className={`text-lg font-semibold text-${color}-400 mb-4`}>Player {index}</h3>
            <div className="space-y-4">
                <div>
                    <label className="block text-xs text-gray-400 mb-1">Provider</label>
                    <select
                        value={agent.kind}
                        onChange={(e) => updateAgent(index, 'kind', e.target.value)}
                        className={`w-full bg-gray-800 border border-gray-600 text-white rounded px-3 py-2 text-sm focus:ring-2 focus:ring-${color}-500 outline-none`}
                    >
                        {AGENT_KINDS.map((k) => (
                            <option key={k} value={k}>{k}</option>
                        ))}
                    </select>
                </div>
                <div>
                    <label className="block text-xs text-gray-400 mb-1">Model Name</label>
                    <input
                        type="text"
                        value={agent.model}
                        onChange={(e) => updateAgent(index, 'model', e.target.value)}
                        className={`w-full bg-gray-800 border border-gray-600 text-white rounded px-3 py-2 text-sm focus:ring-2 focus:ring-${color}-500 outline-none`}
                        placeholder="e.g. gpt-4"
                    />
                </div>
                <div>
                    <label className="block text-xs text-gray-400 mb-1">Secret Profile</label>
                    <select
                        value={agent.secret_profile}
                        onChange={(e) => updateAgent(index, 'secret_profile', e.target.value)}
                        className={`w-full bg-gray-800 border border-gray-600 text-white rounded px-3 py-2 text-sm focus:ring-2 focus:ring-${color}-500 outline-none`}
                    >
                        {availableSecrets.length > 0 ? (
                            availableSecrets.map((s) => (
                                <option key={s} value={s}>{s}</option>
                            ))
                        ) : (
                            <option value="openai.default">openai.default</option>
                        )}
                    </select>
                </div>
            </div>
        </div>
    );

    return (
        <div className="bg-gray-800 p-6 rounded-xl shadow-lg border border-gray-700">
            <h2 className="text-2xl font-bold text-white mb-6 flex items-center">
                <span className="bg-blue-500 w-2 h-8 mr-3 rounded-full"></span>
                Game Configuration (v2)
            </h2>

            <div className="space-y-6">
                {/* Game Selection */}
                <div>
                    <label className="block text-gray-400 text-sm font-medium mb-2">Select Game</label>
                    <div className="grid grid-cols-2 gap-3">
                        {AVAILABLE_GAMES.map((game) => (
                            <button
                                key={game.id}
                                onClick={() => setGameType(game.id)}
                                className={`p-3 rounded-lg text-sm font-medium transition-all duration-200 ${gameType === game.id
                                    ? 'bg-blue-600 text-white shadow-blue-500/30 shadow-lg ring-2 ring-blue-400 ring-offset-2 ring-offset-gray-800'
                                    : 'bg-gray-700 text-gray-300 hover:bg-gray-600'
                                    }`}
                            >
                                {game.name}
                            </button>
                        ))}
                    </div>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                    {renderAgentConfig(agent1, 1, 'blue')}
                    {renderAgentConfig(agent2, 2, 'purple')}
                </div>

                <button
                    onClick={handleStart}
                    disabled={isLoading}
                    className={`w-full py-4 rounded-xl font-bold text-lg uppercase tracking-wider transition-all duration-200 ${isLoading
                        ? 'bg-gray-600 text-gray-400 cursor-not-allowed'
                        : 'bg-gradient-to-r from-blue-600 to-purple-600 hover:from-blue-500 hover:to-purple-500 text-white shadow-lg shadow-blue-500/20 transform hover:-translate-y-0.5'
                        }`}
                >
                    {isLoading ? 'Starting Arena...' : 'Start Battle'}
                </button>
            </div>
        </div>
    );
};
