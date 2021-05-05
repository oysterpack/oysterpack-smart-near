
module.exports = function getConfig(contractName) {
	let config = {
		networkId: 'default',
		nodeUrl: 'https://rpc.testnet.near.org',
		walletUrl: 'https://wallet.testnet.near.org',
		helperUrl: 'https://helper.testnet.near.org',
		contractName,
	};
    
	if (process.env.APP_ENV !== undefined) {
		config = {
			...config,
			GAS: '300000000000000',
			DEFAULT_NEW_ACCOUNT_AMOUNT: '10',
			contractMethods: {
				changeMethods: ['new', 'create', 'purchase'],
				viewMethods: ['get_message'],
			},
		};
	}

	return config;
};
