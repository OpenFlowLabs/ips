// This Jenkinsfile is only used for illumos builds.
// For all other CI workflows, GitHub Actions is used (see .github/workflows/rust.yml)
pipeline {
    agent {
        node {
            label 'buildserver'
        }
    }

    options {
        buildDiscarder logRotator(
            daysToKeepStr: '1',
            numToKeepStr: '3'
        )
    }

    stages {
        stage('Build') {
            steps {
                sh '''
                    source $HOME/.profile
                    cargo version
                    cargo build
                '''
            }
        }
        stage('Test') {
            steps {
                sh '''
                    source $HOME/.profile
                    cargo version
                    cargo test
                '''
            }
        }
    }
}
