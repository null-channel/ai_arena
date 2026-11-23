import React from 'react';

interface GameLogProps {
    logs: any[];
}

export const GameLog: React.FC<GameLogProps> = ({ logs }) => {
    const scrollRef = React.useRef<HTMLDivElement>(null);

    React.useEffect(() => {
        if (scrollRef.current) {
            scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
        }
    }, [logs]);

    return (
        <div className="bg-gray-800 rounded-xl shadow-lg border border-gray-700 flex flex-col h-[600px]">
            <div className="p-4 border-b border-gray-700 bg-gray-800/50 rounded-t-xl">
                <h2 className="text-lg font-bold text-white flex items-center">
                    <span className="w-2 h-2 bg-green-500 rounded-full mr-2 animate-pulse"></span>
                    Battle Log
                </h2>
            </div>

            <div
                ref={scrollRef}
                className="flex-1 overflow-y-auto p-4 space-y-3 font-mono text-sm custom-scrollbar"
            >
                {logs.length === 0 && (
                    <div className="text-gray-500 text-center italic mt-10">
                        Waiting for game to start...
                    </div>
                )}

                {logs.map((log, idx) => (
                    <div
                        key={idx}
                        className={`p-3 rounded border ${log.error
                                ? 'bg-red-900/20 border-red-800 text-red-300'
                                : 'bg-gray-900/50 border-gray-700 text-gray-300'
                            }`}
                    >
                        <div className="flex justify-between items-start mb-1">
                            <span className="text-xs text-gray-500">Turn {log.turn_number || log.round || '?'}</span>
                            <span className={`text-xs px-2 py-0.5 rounded ${log.current_player === 'X' || log.current_player === 'White' || log.winner?.includes('Player 1')
                                    ? 'bg-blue-900/30 text-blue-400'
                                    : 'bg-purple-900/30 text-purple-400'
                                }`}>
                                {log.current_player || 'System'}
                            </span>
                        </div>

                        <pre className="whitespace-pre-wrap overflow-x-auto">
                            {JSON.stringify(log, null, 2)}
                        </pre>
                    </div>
                ))}
            </div>
        </div>
    );
};
