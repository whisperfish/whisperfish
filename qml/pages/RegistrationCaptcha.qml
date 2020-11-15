import QtQuick 2.2
import Sailfish.Silica 1.0

Page {
    id: captchaPage
    objectName: "captchaPage"

    //: Title for captcha web view
    //% "Registration Captcha"
    title: qsTrId("whisperfish-registration-captcha-title")

    Component {
        id: webViewComponent

        SilicaWebView {
            id: webView

            url: "https://signalcaptchas.org/registration/generate.html"

            experimental.overview: true
            experimental.customLayoutWidth: webViewPage.width / (0.5 + QMLUtils.pScale)

            onLoadingChanged: {
                busy = loading
            }
        }
    }
}
