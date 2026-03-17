#ifndef WHISPERFISH_TRANSFER_PLUGIN
#define WHISPERFISH_TRANSFER_PLUGIN

#include "transferplugininterface.h"
#include <QObject>

class Q_DECL_EXPORT WhisperfishTransferPlugin : public QObject, public TransferPluginInterface
{
    Q_OBJECT
	Q_PLUGIN_METADATA(IID "be.rubdos.harbour-whisperfish.transfer.plugin")
	Q_INTERFACES(TransferPluginInterface)

	public:

		WhisperfishTransferPlugin() {};
		~WhisperfishTransferPlugin() {};

		QString pluginId() const;

		MediaTransferInterface* transferObject();
};

#endif
