#include "WhisperfishPluginInfo.h"

WhisperfishPluginInfo::WhisperfishPluginInfo():
    is_ready(false)
{
}

WhisperfishPluginInfo::~WhisperfishPluginInfo()
{
}

QList<SharingMethodInfo> WhisperfishPluginInfo::info() const
{ 
    return infoList;
}

bool WhisperfishPluginInfo::ready() const
{
    return is_ready;
}

void WhisperfishPluginInfo::query()
{
    SharingMethodInfo info;
    QStringList capabilities;


    info.setDisplayName(QLatin1String(APP_NAME));
    info.setMethodId(QLatin1String(PLUGIN_ID));
    info.setMethodIcon(QLatin1String(APP_ICON));
    info.setShareUIPath(QLatin1String("/usr/share/nemo-transferengine/plugins/sharing/WhisperfishShare.qml"));
    info.setSupportsMultipleFiles(true);

    // We just allow everything and hope for bug reports where special handling is needed.
    capabilities << QLatin1String("application/*")
                 << QLatin1String("image/*")
                 << QLatin1String("audio/*")
                 << QLatin1String("video/*")
                 << QLatin1String("text/*")
                 << QLatin1String("*/*");
    info.setCapabilities(capabilities);

    infoList << info;

    is_ready = true;
    emit infoReady();
}
