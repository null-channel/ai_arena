import React from 'react';

interface GameBoardProps {
    gameState: any;
    gameType: string;
}

export const GameBoard: React.FC<GameBoardProps> = ({ gameState, gameType }) => {
    if (!gameState) {
        return (
            <div className="h-full flex items-center justify-center text-gray-500 italic">
                No active game
            </div>
        );
    }

    const renderTicTacToe = () => {
        const board = gameState.board || [['', '', ''], ['', '', ''], ['', '', '']];
        return (
            <div className="grid grid-cols-3 gap-2 w-64 mx-auto">
                {board.map((row: any[], rIdx: number) =>
                    row.map((cell: string | null, cIdx: number) => (
                        <div
                            key={`${rIdx}-${cIdx}`}
                            className={`h-20 w-20 flex items-center justify-center text-4xl font-bold rounded-lg border-2 transition-all duration-300 ${cell === 'X'
                                    ? 'border-blue-500 bg-blue-900/20 text-blue-400 shadow-[0_0_15px_rgba(59,130,246,0.5)]'
                                    : cell === 'O'
                                        ? 'border-purple-500 bg-purple-900/20 text-purple-400 shadow-[0_0_15px_rgba(168,85,247,0.5)]'
                                        : 'border-gray-700 bg-gray-800'
                                }`}
                        >
                            {cell}
                        </div>
                    ))
                )}
            </div>
        );
    };

    const renderConnectFour = () => {
        const board = gameState.board || [];
        // Connect Four is usually 6 rows x 7 cols
        return (
            <div className="bg-blue-900 p-4 rounded-lg shadow-2xl inline-block mx-auto">
                <div className="grid grid-rows-6 gap-2">
                    {board.map((row: any[], rIdx: number) => (
                        <div key={rIdx} className="grid grid-cols-7 gap-2">
                            {row.map((cell: string | null, cIdx: number) => (
                                <div
                                    key={`${rIdx}-${cIdx}`}
                                    className={`h-12 w-12 rounded-full border-4 border-blue-800 shadow-inner ${cell === 'Red'
                                            ? 'bg-red-500 shadow-[inset_0_2px_5px_rgba(0,0,0,0.5)]'
                                            : cell === 'Yellow'
                                                ? 'bg-yellow-400 shadow-[inset_0_2px_5px_rgba(0,0,0,0.5)]'
                                                : 'bg-gray-900'
                                        }`}
                                />
                            ))}
                        </div>
                    ))}
                </div>
            </div>
        );
    };

    const renderChess = () => {
        // For now, just display the FEN string and maybe a simple representation
        // A full chess board renderer is complex, so we'll stick to a basic view or text for MVP
        return (
            <div className="text-center space-y-4">
                <div className="bg-gray-900 p-4 rounded font-mono text-sm break-all border border-gray-700">
                    <div className="text-gray-500 text-xs mb-1">FEN Position</div>
                    {gameState.fen}
                </div>
                <div className="text-gray-400 italic">
                    (Chess board visualization coming soon)
                </div>
            </div>
        );
    };

    const renderRPS = () => {
        const lastRound = gameState.history ? gameState.history[gameState.history.length - 1] : null;
        return (
            <div className="text-center space-y-8">
                <div className="flex justify-center items-center space-x-12">
                    <div className="text-center">
                        <div className="text-blue-400 font-bold mb-2">Player 1</div>
                        <div className="text-6xl">{gameState.scores?.player_one_score || 0}</div>
                    </div>
                    <div className="text-gray-600 text-2xl font-bold">VS</div>
                    <div className="text-center">
                        <div className="text-purple-400 font-bold mb-2">Player 2</div>
                        <div className="text-6xl">{gameState.scores?.player_two_score || 0}</div>
                    </div>
                </div>

                {lastRound && (
                    <div className="bg-gray-800 p-6 rounded-xl border border-gray-700 animate-fade-in">
                        <div className="text-gray-400 mb-4">Round {lastRound.round_number} Result</div>
                        <div className="flex justify-center items-center space-x-8 text-4xl">
                            <span>{lastRound.player_one_choice || '?'}</span>
                            <span className="text-gray-600">⚡</span>
                            <span>{lastRound.player_two_choice || '?'}</span>
                        </div>
                        <div className="mt-4 text-xl font-bold text-white">
                            {lastRound.winner === 0 ? 'Player 1 Wins!' : lastRound.winner === 1 ? 'Player 2 Wins!' : 'Draw!'}
                        </div>
                    </div>
                )}
            </div>
        );
    };

    return (
        <div className="bg-gray-800 p-8 rounded-xl shadow-lg border border-gray-700 h-[600px] flex flex-col items-center justify-center relative overflow-hidden">
            <div className="absolute top-0 left-0 w-full h-1 bg-gradient-to-r from-blue-500 via-purple-500 to-pink-500"></div>

            {gameType === 'TicTacToe' && renderTicTacToe()}
            {gameType === 'ConnectFour' && renderConnectFour()}
            {gameType === 'Chess' && renderChess()}
            {gameType === 'RockPaperScissors' && renderRPS()}

            {gameState.game_over && (
                <div className="absolute inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-10">
                    <div className="bg-gray-900 p-8 rounded-2xl border-2 border-yellow-500/50 shadow-[0_0_50px_rgba(234,179,8,0.2)] text-center transform scale-110">
                        <h3 className="text-3xl font-black text-transparent bg-clip-text bg-gradient-to-r from-yellow-400 to-orange-500 mb-2">
                            GAME OVER
                        </h3>
                        <div className="text-xl text-white">
                            Winner: <span className="font-bold text-yellow-400">{gameState.winner || 'Draw'}</span>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
};
