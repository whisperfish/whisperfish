import QtQuick 2.2
import Sailfish.Silica 1.0

Page {
    id: aboutpage
    objectName: "aboutPage"

    SilicaFlickable {
        anchors.fill: parent
        contentWidth: parent.width
        contentHeight: col.height + Theme.paddingLarge

        VerticalScrollDecorator {}

        Column {
            id: col
            spacing: Theme.paddingLarge
            width: parent.width
            PageHeader {
                //: Title for about page
                //% "About Whisperfish"
                title: qsTrId("whisperfish-about")
            }

            Image {
                anchors.horizontalCenter: parent.horizontalCenter
                source: "/usr/share/icons/hicolor/172x172/apps/harbour-whisperfish.png"
            }

            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                font.bold: true
                //: Whisperfish version string
                //% "Whisperfish v%1"
                text: qsTrId("whisperfish-version").arg(Qt.application.version)
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                //: Whisperfish description
                //% "Signal client for Sailfish OS"
                text: qsTrId("whisperfish-description")
            }

            SectionHeader {
                //: Description
                //% "Description"
                text: qsTrId("whisperfish-description-section")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                //: Whisperfish description, longer version, also for Jolla Store
                //% "Whisperfish is an unofficial, but advanced Signal client for Sailfish OS. "
                //% "Whisperfish is highly usable, but is still considered beta quality software. "
                //% "Make sure to update regularily! Also, check our Wiki and feel free to contribute to it! "
                //% "Do not ever contact the Signal developers about a Whisperfish issue, contact us instead!"
                text: qsTrId("whisperfish-long-description")
            }

            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                wrapMode: Text.Wrap
                text: {
                    var build_id = CiJobUrl ?
                        "<a href=\"" + CiJobUrl + "\">" + LongAppVersion + "</a>"
                        : LongAppVersion ;
                    //: Whisperfish long version string and build ID
                    //% "Build ID: %1"
                    qsTrId("whisperfish-build-id").arg(build_id)
                }
                textFormat: Text.StyledText
                onLinkActivated: Qt.openUrlExternally(link)
                linkColor: Theme.primaryColor
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Support on Liberapay
                //% "Support on Liberapay"
                text: qsTrId("whisperfish-liberapay")
                onClicked: {
                    Qt.openUrlExternally("https://liberapay.com/rubdos/")
                }
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Source Code
                //% "Source Code"
                text: qsTrId("whisperfish-source-code")
                onClicked: {
                    Qt.openUrlExternally("https://gitlab.com/whisperfish/whisperfish")
                }
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Report a Bug
                //% "Report a Bug"
                text: qsTrId("whisperfish-bug-report")
                onClicked: {
                    Qt.openUrlExternally("https://gitlab.com/whisperfish/whisperfish/issues")
                }
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                //: Visit the Wiki button, tapping links to the Whisperfish Wiki
                //% "Visit the Wiki"
                text: qsTrId("whisperfish-about-wiki-link")
                onClicked: {
                    Qt.openUrlExternally("https://gitlab.com/whisperfish/whisperfish/-/wikis/home")
                }
            }

            SectionHeader {
                //: Copyright
                //% "Copyright"
                text: qsTrId("whisperfish-copyright")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Andrew E. Bruno (2016-2018)\n" +
                    "Ruben De Smet (2019-2023)\n" +
                    "Matti \"direc85\" Viljanen (2021-2023)\n" +
                    "Markus Törnqvist (2019-2021)\n" +
                    "Mirian Margiani (2021-2023)\n" +
                    "Gabriel Margiani (2021-2022)\n" +
                    "Thomas Michel (2021)\n" +
                    "License: AGPLv3 & GPLv3"
                }
            }

            SectionHeader {
                //: Additional Copyright
                //% "Additional Copyright"
                text: qsTrId("whisperfish-extra-copyright")
            }

            Label {
                text: "libsignal-client by Signal"
                anchors.horizontalCenter: parent.horizontalCenter
                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                width: (parent ? parent.width : Screen.width) - Theme.paddingLarge * 2
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: Text.AlignLeft
                x: Theme.paddingLarge
            }

            Label {
                text: "libsignal-service-rs by Ruben De Smet, Gabriel Féron, and Michael Bryan"
                anchors.horizontalCenter: parent.horizontalCenter
                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                width: (parent ? parent.width : Screen.width) - Theme.paddingLarge * 2
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: Text.AlignLeft
                x: Theme.paddingLarge
            }

            SectionHeader {
                //: Translators heading in About page
                //% "Translators"
                text: qsTrId("whisperfish-translators")
            }

            /// BEGIN TRANSLATORS
            SectionHeader {
                //: Norwegian Bokmål (nb_NO) language about page translation section
                //% "Norwegian Bokmål translators"
                text: qsTrId("whisperfish-translators-nb_NO")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Allan Nordhøy"
                }
            }

            SectionHeader {
                //: Dutch (Belgium) (nl_BE) language about page translation section
                //% "Dutch (Belgium) translators"
                text: qsTrId("whisperfish-translators-nl_BE")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Ruben De Smet" + "\n" +
                    "Nathan" + "\n" +
                    "Alexander Schlarb" + "\n" +
                    "J. Lavoie" + "\n" +
                    "carlosgonz"
                }
            }

            SectionHeader {
                //: Lithuanian (lt) language about page translation section
                //% "Lithuanian translators"
                text: qsTrId("whisperfish-translators-lt")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Gediminas Murauskas"
                }
            }

            SectionHeader {
                //: Dutch (nl) language about page translation section
                //% "Dutch translators"
                text: qsTrId("whisperfish-translators-nl")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Ruben De Smet" + "\n" +
                    "Nathan" + "\n" +
                    "Alexander Schlarb" + "\n" +
                    "J. Lavoie" + "\n" +
                    "carlosgonz"
                }
            }

            SectionHeader {
                //: Italian (it) language about page translation section
                //% "Italian translators"
                text: qsTrId("whisperfish-translators-it")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "J. Lavoie" + "\n" +
                    "Andrea Scarpino"
                }
            }

            SectionHeader {
                //: German (de) language about page translation section
                //% "German translators"
                text: qsTrId("whisperfish-translators-de")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "J. Lavoie" + "\n" +
                    "PawelSpoon" + "\n" +
                    "Yo" + "\n" +
                    "Peter G" + "\n" +
                    "Stephan Lohse" + "\n" +
                    "Alexander Schlarb" + "\n" +
                    "Simon Hahne" + "\n" +
                    "Ettore Atalan" + "\n" +
                    "Sebastian Maus" + "\n" +
                    "carlosgonz" + "\n" +
                    "tct123"
                }
            }

            SectionHeader {
                //: French (fr) language about page translation section
                //% "French translators"
                text: qsTrId("whisperfish-translators-fr")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "J. Lavoie" + "\n" +
                    "Alexander Schlarb" + "\n" +
                    "Bérenger" + "\n" +
                    "Eric" + "\n" +
                    "Thibaut Vandervelden" + "\n" +
                    "carlosgonz"
                }
            }

            SectionHeader {
                //: Finnish (fi) language about page translation section
                //% "Finnish translators"
                text: qsTrId("whisperfish-translators-fi")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Matti Viljanen" + "\n" +
                    "J. Lavoie" + "\n" +
                    "Tuomas F Nyqvist" + "\n" +
                    "jmcwine" + "\n" +
                    "Alexander Schlarb" + "\n" +
                    "Elmeri Länsiharju" + "\n" +
                    "Ricky Tigg" + "\n" +
                    "carlosgonz"
                }
            }

            SectionHeader {
                //: Slovenian (sl) language about page translation section
                //% "Slovenian translators"
                text: qsTrId("whisperfish-translators-sl")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Jože Prijatelj"
                }
            }

            SectionHeader {
                //: Swedish (sv) language about page translation section
                //% "Swedish translators"
                text: qsTrId("whisperfish-translators-sv")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "bittin1ddc447d824349b2" + "\n" +
                    "Tuomas F Nyqvist" + "\n" +
                    "Luna Jernberg" + "\n" +
                    "Åke Engelbrektson" + "\n" +
                    "fluffysfriends"
                }
            }

            SectionHeader {
                //: Polish (pl) language about page translation section
                //% "Polish translators"
                text: qsTrId("whisperfish-translators-pl")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "A" + "\n" +
                    "Karol Kurek" + "\n" +
                    "Alexander Schlarb" + "\n" +
                    "Eryk Michalak" + "\n" +
                    "Wojtek" + "\n" +
                    "carlosgonz" + "\n" +
                    "kan-ibal"
                }
            }

            SectionHeader {
                //: Chinese (Simplified) (zh_CN) language about page translation section
                //% "Chinese (Simplified) translators"
                text: qsTrId("whisperfish-translators-zh_CN")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "dashinfantry" + "\n" +
                    "Alexander Schlarb" + "\n" +
                    "Rui Kon" + "\n" +
                    "carlosgonz"
                }
            }

            SectionHeader {
                //: Hungarian (hu) language about page translation section
                //% "Hungarian translators"
                text: qsTrId("whisperfish-translators-hu")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Sz. G" + "\n" +
                    "Alexander Schlarb" + "\n" +
                    "carlosgonz"
                }
            }

            SectionHeader {
                //: Portuguese (Portugal) (pt_PT) language about page translation section
                //% "Portuguese (Portugal) translators"
                text: qsTrId("whisperfish-translators-pt_PT")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Júlio" + "\n" +
                    "Antonio Maretzek" + "\n" +
                    "J. Lavoie" + "\n" +
                    "ssantos"
                }
            }

            SectionHeader {
                //: Czech (cs) language about page translation section
                //% "Czech translators"
                text: qsTrId("whisperfish-translators-cs")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "PawelSpoon"
                }
            }

            SectionHeader {
                //: Catalan (ca) language about page translation section
                //% "Catalan translators"
                text: qsTrId("whisperfish-translators-ca")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Jaume"
                }
            }

            SectionHeader {
                //: Russian (ru) language about page translation section
                //% "Russian translators"
                text: qsTrId("whisperfish-translators-ru")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Nikolai Sinyov" + "\n" +
                    "Николай Синёв"
                }
            }

            SectionHeader {
                //: Portuguese (Brazil) (pt_BR) language about page translation section
                //% "Portuguese (Brazil) translators"
                text: qsTrId("whisperfish-translators-pt_BR")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "J. Lavoie" + "\n" +
                    "Caio 2k"
                }
            }

            SectionHeader {
                //: Romanian (ro) language about page translation section
                //% "Romanian translators"
                text: qsTrId("whisperfish-translators-ro")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Florin Voicu" + "\n" +
                    "Vlad G"
                }
            }

            SectionHeader {
                //: Turkish (tr) language about page translation section
                //% "Turkish translators"
                text: qsTrId("whisperfish-translators-tr")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Oğuz Ersen"
                }
            }

            SectionHeader {
                //: Greek (el) language about page translation section
                //% "Greek translators"
                text: qsTrId("whisperfish-translators-el")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Chris" + "\n" +
                    "J. Lavoie"
                }
            }

            SectionHeader {
                //: Basque (eu) language about page translation section
                //% "Basque translators"
                text: qsTrId("whisperfish-translators-eu")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "Sergio Varela"
                }
            }

            SectionHeader {
                //: Spanish (es) language about page translation section
                //% "Spanish translators"
                text: qsTrId("whisperfish-translators-es")
            }

            TextArea {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width
                horizontalAlignment: TextEdit.Center
                readOnly: true
                text: {
                    "gallegonovato" + "\n" +
                    "J. Lavoie" + "\n" +
                    "Jose Manuel REGUEIRA" + "\n" +
                    "Alexander Schlarb" + "\n" +
                    "PawelSpoon" + "\n" +
                    "carlosgonz" + "\n" +
                    "Allan Nordhøy"
                }
            }
            /// END TRANSLATORS
        }
    }
}
